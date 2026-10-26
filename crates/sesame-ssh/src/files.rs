//! A file manager confined to one site's directory, over SFTP.
//!
//! Every path is checked twice: lexically (no `..` can climb above the root)
//! and on the server (`realpath`, so a symlink cannot lead outside). Files
//! written through it are handed to the site's owner afterwards.

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use russh_sftp::client::SftpSession;
use sesame_core::ServerConfig;

use crate::{Exec, Session};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub is_link: bool,
    pub size: u64,
    /// Seconds since the epoch.
    pub modified: u32,
    pub mode: u32,
}

/// `root` joined with a path relative to it, normalised, or None if it would
/// leave `root`. Absolute `rel` paths are treated as relative to `root`.
pub fn jail(root: &str, rel: &str) -> Option<String> {
    let root = root.trim_end_matches('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p if p.contains('\0') => return None,
            p => parts.push(p),
        }
    }
    Some(if parts.is_empty() { root.to_string() } else { format!("{root}/{}", parts.join("/")) })
}

/// The parent of a relative path ("" for the root).
pub fn parent(rel: &str) -> String {
    let rel = rel.trim_matches('/');
    rel.rsplit_once('/').map(|(p, _)| p.to_string()).unwrap_or_default()
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub struct SiteFiles {
    session: Arc<Session>,
    sftp: SftpSession,
    root: String,
    owner: String,
}

impl SiteFiles {
    pub async fn open(server: &ServerConfig, root: &str) -> Result<Self> {
        if !root.starts_with('/') || root.len() < 2 {
            bail!("invalid site root");
        }
        let session = Arc::new(Session::connect(server).await?);
        let channel = session.handle.channel_open_session().await?;
        channel.request_subsystem(true, "sftp").await?;
        let sftp = SftpSession::new(channel.into_stream()).await?;
        let root = sftp.canonicalize(root).await.context("the site directory does not exist")?;
        let owner = session.exec(&format!("stat -c %U {}", quote(&root))).await?.stdout.trim().to_string();
        if owner.is_empty() {
            bail!("cannot read the owner of {root}");
        }
        Ok(Self { session, sftp, root, owner })
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// Absolute path for `rel`, verified to stay inside the root on the server too.
    async fn resolve(&self, rel: &str, must_exist: bool) -> Result<String> {
        let path = jail(&self.root, rel).context("path leaves the site directory")?;
        let real = if must_exist {
            self.sftp.canonicalize(path.as_str()).await?
        } else {
            // New entries: check the parent instead.
            let (dir, name) = path.rsplit_once('/').context("bad path")?;
            let real_dir = self.sftp.canonicalize(if dir.is_empty() { "/" } else { dir }).await?;
            format!("{}/{name}", real_dir.trim_end_matches('/'))
        };
        if real != self.root && !real.starts_with(&format!("{}/", self.root)) {
            bail!("path leaves the site directory");
        }
        Ok(path)
    }

    pub async fn list(&self, rel: &str) -> Result<Vec<Entry>> {
        let dir = self.resolve(rel, true).await?;
        let mut out: Vec<Entry> = self
            .sftp
            .read_dir(dir.as_str())
            .await?
            .map(|e| {
                let m = e.metadata();
                Entry {
                    name: e.file_name(),
                    is_dir: m.is_dir(),
                    is_link: m.is_symlink(),
                    size: m.size.unwrap_or(0),
                    modified: m.mtime.unwrap_or(0),
                    mode: m.permissions.unwrap_or(0) & 0o7777,
                }
            })
            .filter(|e| e.name != "." && e.name != "..")
            .collect();
        out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        Ok(out)
    }

    /// Reads a file of at most `max` bytes.
    pub async fn read(&self, rel: &str, max: u64) -> Result<Vec<u8>> {
        let path = self.resolve(rel, true).await?;
        let meta = self.sftp.metadata(path.as_str()).await?;
        if meta.is_dir() {
            bail!("{rel} is a directory");
        }
        if meta.size.unwrap_or(0) > max {
            bail!("{rel} is larger than {} KB", max / 1024);
        }
        Ok(self.sftp.read(path.as_str()).await?)
    }

    async fn give_to_owner(&self, path: &str) -> Result<()> {
        let out = self.session.exec(&format!("chown -h {o}:{o} {p}", o = quote(&self.owner), p = quote(path))).await?;
        if !out.success() {
            bail!("chown failed: {}", out.stderr.trim());
        }
        Ok(())
    }

    pub async fn write(&self, rel: &str, data: &[u8]) -> Result<()> {
        let path = self.resolve(rel, false).await?;
        // `create` truncates or creates, so both new and existing files work.
        let mut f = self.sftp.create(path.as_str()).await?;
        tokio::io::AsyncWriteExt::write_all(&mut f, data).await?;
        tokio::io::AsyncWriteExt::shutdown(&mut f).await?;
        self.give_to_owner(&path).await
    }

    pub async fn mkdir(&self, rel: &str) -> Result<()> {
        let path = self.resolve(rel, false).await?;
        self.sftp.create_dir(path.as_str()).await?;
        self.give_to_owner(&path).await
    }

    pub async fn rename(&self, from: &str, to: &str) -> Result<()> {
        let a = self.resolve(from, true).await?;
        let b = self.resolve(to, false).await?;
        Ok(self.sftp.rename(a.as_str(), b.as_str()).await?)
    }

    /// Removes a file, or a directory with everything in it.
    pub async fn remove(&self, rel: &str) -> Result<()> {
        let path = self.resolve(rel, true).await?;
        if path == self.root {
            bail!("refusing to delete the site directory itself");
        }
        let out = self.session.exec(&format!("rm -rf -- {}", quote(&path))).await?;
        if !out.success() {
            bail!("delete failed: {}", out.stderr.trim());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jail_keeps_paths_inside() {
        let r = "/home/s_shop/shop.test";
        assert_eq!(jail(r, "").as_deref(), Some(r));
        assert_eq!(jail(r, "public/index.php").as_deref(), Some("/home/s_shop/shop.test/public/index.php"));
        assert_eq!(jail(r, "/etc/passwd").as_deref(), Some("/home/s_shop/shop.test/etc/passwd"));
        assert_eq!(jail(r, "a/../b").as_deref(), Some("/home/s_shop/shop.test/b"));
        assert_eq!(jail(r, ".."), None);
        assert_eq!(jail(r, "a/../../x"), None);
        assert_eq!(jail(r, "../s_other/.env"), None);
        assert_eq!(parent("public/css/app.css"), "public/css");
        assert_eq!(parent("public"), "");
    }
}
