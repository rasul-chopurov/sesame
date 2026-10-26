//! Local state. A trait so the SaaS backend can swap SQLite for Postgres
//! without touching callers.

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use crate::{ServerConfig, ServerId, ServerSnapshot};

pub trait Store: Send + Sync {
    fn servers(&self) -> Result<Vec<ServerConfig>>;
    fn save_server(&self, server: &ServerConfig) -> Result<()>;
    fn delete_server(&self, id: ServerId) -> Result<()>;
    fn snapshot(&self, id: ServerId) -> Result<Option<ServerSnapshot>>;
    fn save_snapshot(&self, id: ServerId, snapshot: &ServerSnapshot) -> Result<()>;
    fn setting(&self, key: &str) -> Result<Option<String>>;
    fn set_setting(&self, key: &str, value: &str) -> Result<()>;
    fn log_audit(&self, entry: &AuditEntry) -> Result<()>;
    /// The repository a site deploys from.
    fn site_link(&self, server: ServerId, domain: &str) -> Result<Option<SiteLink>>;
    fn set_site_link(&self, link: &SiteLink) -> Result<()>;
    fn delete_site_link(&self, server: ServerId, domain: &str) -> Result<()>;
    fn audit(&self, server: Option<ServerId>, limit: usize) -> Result<Vec<AuditEntry>>;
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SiteLink {
    pub server_id: ServerId,
    pub domain: String,
    pub app_path: String,
    pub repo: String,
    pub branch: String,
}

/// One change Sesame made (or tried to make) on a server. Never contains secrets.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AuditEntry {
    /// RFC 3339.
    pub at: String,
    pub server_id: ServerId,
    pub op: String,
    pub title: String,
    /// `user`, or `agent:<token name>` for API/MCP calls.
    pub actor: String,
    pub ok: bool,
    /// Tail of the output, for troubleshooting.
    pub log: String,
}

pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    /// Opens `state.db` in `$SESAME_DATA_DIR`, else the platform data
    /// directory (`~/Library/Application Support/dev.sesame.Sesame` on macOS).
    pub fn open_default() -> Result<Self> {
        let dir = Self::data_dir()?;
        std::fs::create_dir_all(&dir)?;
        Self::open(&dir.join("state.db"))
    }

    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::migrate(&conn)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn migrate(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS servers (
                id TEXT PRIMARY KEY,
                position INTEGER NOT NULL,
                config TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS snapshots (
                server_id TEXT PRIMARY KEY REFERENCES servers(id) ON DELETE CASCADE,
                taken_at TEXT NOT NULL,
                data TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS audit (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                at TEXT NOT NULL,
                server_id TEXT NOT NULL,
                op TEXT NOT NULL,
                title TEXT NOT NULL,
                actor TEXT NOT NULL,
                ok INTEGER NOT NULL,
                log TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS site_links (
                server_id TEXT NOT NULL,
                domain TEXT NOT NULL,
                app_path TEXT NOT NULL,
                repo TEXT NOT NULL,
                branch TEXT NOT NULL,
                PRIMARY KEY (server_id, domain)
            );
            CREATE TABLE IF NOT EXISTS vault_meta (
                key TEXT PRIMARY KEY,
                value BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS vault (
                scope TEXT NOT NULL,
                name TEXT NOT NULL,
                nonce BLOB NOT NULL,
                ciphertext BLOB NOT NULL,
                PRIMARY KEY (scope, name)
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            PRAGMA foreign_keys = ON;",
        )?;
        Ok(())
    }
}

impl Store for SqliteStore {
    fn servers(&self) -> Result<Vec<ServerConfig>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT config FROM servers ORDER BY position, rowid")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.map(|json| Ok(serde_json::from_str(&json?)?)).collect()
    }

    fn save_server(&self, server: &ServerConfig) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let json = serde_json::to_string(server)?;
        conn.execute(
            "INSERT INTO servers (id, position, config)
             VALUES (?1, (SELECT COALESCE(MAX(position), 0) + 1 FROM servers), ?2)
             ON CONFLICT(id) DO UPDATE SET config = excluded.config",
            params![server.id.to_string(), json],
        )?;
        Ok(())
    }

    fn delete_server(&self, id: ServerId) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM snapshots WHERE server_id = ?1", [id.to_string()])?;
        conn.execute("DELETE FROM servers WHERE id = ?1", [id.to_string()])?;
        Ok(())
    }

    fn snapshot(&self, id: ServerId) -> Result<Option<ServerSnapshot>> {
        let conn = self.conn.lock().unwrap();
        let json: Option<String> = conn
            .query_row(
                "SELECT data FROM snapshots WHERE server_id = ?1",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        json.map(|j| Ok(serde_json::from_str(&j)?)).transpose()
    }

    fn save_snapshot(&self, id: ServerId, snapshot: &ServerSnapshot) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO snapshots (server_id, taken_at, data) VALUES (?1, ?2, ?3)
             ON CONFLICT(server_id) DO UPDATE SET taken_at = excluded.taken_at, data = excluded.data",
            params![id.to_string(), snapshot.taken_at, serde_json::to_string(snapshot)?],
        )?;
        Ok(())
    }

    fn setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| row.get(0))
            .optional()?)
    }

    fn site_link(&self, server: ServerId, domain: &str) -> Result<Option<SiteLink>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT app_path, repo, branch FROM site_links WHERE server_id = ?1 AND domain = ?2",
                params![server.to_string(), domain],
                |r| {
                    Ok(SiteLink {
                        server_id: server,
                        domain: domain.to_string(),
                        app_path: r.get(0)?,
                        repo: r.get(1)?,
                        branch: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    fn set_site_link(&self, l: &SiteLink) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO site_links (server_id, domain, app_path, repo, branch) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(server_id, domain) DO UPDATE SET app_path = excluded.app_path, repo = excluded.repo, branch = excluded.branch",
            params![l.server_id.to_string(), l.domain, l.app_path, l.repo, l.branch],
        )?;
        Ok(())
    }

    fn delete_site_link(&self, server: ServerId, domain: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM site_links WHERE server_id = ?1 AND domain = ?2", params![server.to_string(), domain])?;
        Ok(())
    }

    fn log_audit(&self, e: &AuditEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO audit (at, server_id, op, title, actor, ok, log) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![e.at, e.server_id.to_string(), e.op, e.title, e.actor, e.ok, e.log],
        )?;
        Ok(())
    }

    fn audit(&self, server: Option<ServerId>, limit: usize) -> Result<Vec<AuditEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT at, server_id, op, title, actor, ok, log FROM audit
             WHERE ?1 IS NULL OR server_id = ?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![server.map(|s| s.to_string()), limit as i64], |r| {
            Ok(AuditEntry {
                at: r.get(0)?,
                server_id: r.get::<_, String>(1)?.parse().unwrap_or_default(),
                op: r.get(2)?,
                title: r.get(3)?,
                actor: r.get(4)?,
                ok: r.get(5)?,
                log: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

/// Raw access for the vault: it stores only ciphertext here.
impl SqliteStore {
    pub(crate) fn vault_meta(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row("SELECT value FROM vault_meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub(crate) fn set_vault_meta(&self, key: &str, value: &[u8]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO vault_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub(crate) fn vault_put(&self, scope: &str, name: &str, nonce: &[u8], ct: &[u8]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO vault (scope, name, nonce, ciphertext) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(scope, name) DO UPDATE SET nonce = excluded.nonce, ciphertext = excluded.ciphertext",
            params![scope, name, nonce, ct],
        )?;
        Ok(())
    }

    pub(crate) fn vault_get(&self, scope: &str, name: &str) -> Result<Option<(Vec<u8>, Vec<u8>)>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT nonce, ciphertext FROM vault WHERE scope = ?1 AND name = ?2",
                params![scope, name],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
    }

    pub(crate) fn vault_delete(&self, scope: &str, name: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        match name {
            Some(n) => conn.execute("DELETE FROM vault WHERE scope = ?1 AND name = ?2", params![scope, n])?,
            None => conn.execute("DELETE FROM vault WHERE scope = ?1", params![scope])?,
        };
        Ok(())
    }

    /// Copies the database (and the vault key file, if any) to
    /// `<data dir>/backups/<timestamp>/` before anything destructive.
    pub fn backup_data_dir(reason: &str) -> Result<std::path::PathBuf> {
        let dir = Self::data_dir()?;
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let out = dir.join("backups").join(format!("{stamp}-{reason}"));
        std::fs::create_dir_all(&out)?;
        for name in ["state.db", "vault.key"] {
            let src = dir.join(name);
            if src.exists() {
                std::fs::copy(&src, out.join(name))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(out.join(name), std::fs::Permissions::from_mode(0o600))?;
                }
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(dir.join("backups"), std::fs::Permissions::from_mode(0o700));
        }
        Ok(out)
    }

    pub(crate) fn vault_reset(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("DELETE FROM vault; DELETE FROM vault_meta;")?;
        Ok(())
    }

    pub(crate) fn vault_all(&self) -> Result<Vec<(String, String, Vec<u8>, Vec<u8>)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT scope, name, nonce, ciphertext FROM vault")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Where the database lives (for the key file next to it).
    ///
    /// Test builds never see the real directory: they get a private temp
    /// folder, so no test can touch a user's database or key file.
    pub fn data_dir() -> Result<std::path::PathBuf> {
        if cfg!(test) || std::env::var_os("SESAME_TEST_ISOLATION").is_some() {
            let dir = std::env::temp_dir().join(format!("sesame-test-{}", std::process::id()));
            std::fs::create_dir_all(&dir)?;
            return Ok(dir);
        }
        Ok(match std::env::var_os("SESAME_DATA_DIR") {
            Some(d) => std::path::PathBuf::from(d),
            None => directories::ProjectDirs::from("dev", "sesame", "Sesame")
                .context("no home directory")?
                .data_dir()
                .to_path_buf(),
        })
    }
}

impl SqliteStore {
    pub fn setting_or(&self, key: &str, default: &str) -> String {
        self.setting(key).ok().flatten().unwrap_or_else(|| default.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servers_round_trip_in_insertion_order() {
        let store = SqliteStore::in_memory().unwrap();
        let a = ServerConfig::new("a", "1.1.1.1", "root");
        let mut b = ServerConfig::new("b", "2.2.2.2", "deploy");
        store.save_server(&a).unwrap();
        store.save_server(&b).unwrap();
        b.name = "b2".into();
        store.save_server(&b).unwrap();

        let all = store.servers().unwrap();
        assert_eq!(all, vec![a.clone(), b.clone()]);

        store.delete_server(a.id).unwrap();
        assert_eq!(store.servers().unwrap(), vec![b]);
    }

    #[test]
    fn audit_newest_first_per_server() {
        let store = SqliteStore::in_memory().unwrap();
        let (a, b) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        for (i, s) in [(1, a), (2, b), (3, a)] {
            store
                .log_audit(&AuditEntry {
                    at: format!("t{i}"),
                    server_id: s,
                    op: "service_action".into(),
                    title: format!("op {i}"),
                    actor: "user".into(),
                    ok: true,
                    log: String::new(),
                })
                .unwrap();
        }
        let only_a: Vec<String> = store.audit(Some(a), 10).unwrap().into_iter().map(|e| e.title).collect();
        assert_eq!(only_a, ["op 3", "op 1"]);
        assert_eq!(store.audit(None, 2).unwrap().len(), 2);
    }

    #[test]
    fn site_links_upsert() {
        let store = SqliteStore::in_memory().unwrap();
        let id = uuid::Uuid::new_v4();
        let mut l = SiteLink { server_id: id, domain: "shop.test".into(), app_path: "/var/www/shop".into(), repo: "a/shop".into(), branch: "main".into() };
        store.set_site_link(&l).unwrap();
        l.branch = "release".into();
        store.set_site_link(&l).unwrap();
        assert_eq!(store.site_link(id, "shop.test").unwrap(), Some(l));
        store.delete_site_link(id, "shop.test").unwrap();
        assert!(store.site_link(id, "shop.test").unwrap().is_none());
    }

    #[test]
    fn settings_upsert() {
        let store = SqliteStore::in_memory().unwrap();
        assert_eq!(store.setting("theme").unwrap(), None);
        store.set_setting("theme", "dark").unwrap();
        store.set_setting("theme", "light").unwrap();
        assert_eq!(store.setting_or("theme", "system"), "light");
        assert_eq!(store.setting_or("lang", "ru"), "ru");
    }

    #[test]
    fn snapshot_replaces_previous() {
        let store = SqliteStore::in_memory().unwrap();
        let s = ServerConfig::new("a", "1.1.1.1", "root");
        store.save_server(&s).unwrap();
        assert!(store.snapshot(s.id).unwrap().is_none());

        let mut snap = ServerSnapshot { taken_at: "t1".into(), ..Default::default() };
        store.save_snapshot(s.id, &snap).unwrap();
        snap.taken_at = "t2".into();
        snap.os.hostname = "box".into();
        store.save_snapshot(s.id, &snap).unwrap();
        assert_eq!(store.snapshot(s.id).unwrap(), Some(snap));
    }
}
