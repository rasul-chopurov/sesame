//! GitHub REST client for connecting sites to repositories.
//!
//! Uses a personal access token (fine-grained: Contents read, Metadata read,
//! Administration write for deploy keys) kept in the encrypted vault.

use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

const API: &str = "https://api.github.com";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct User {
    pub login: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Repo {
    pub full_name: String,
    pub private: bool,
    pub default_branch: String,
    pub ssh_url: String,
    #[serde(default)]
    pub pushed_at: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Branch {
    pub name: String,
    pub commit: CommitRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CommitRef {
    pub sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Commit {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub date: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DeployKey {
    pub id: u64,
    pub key: String,
    pub title: String,
    pub read_only: bool,
}

pub struct Client {
    token: String,
    http: reqwest::Client,
}

/// `owner/name`, both parts safe for URLs and shell.
pub fn valid_repo(full_name: &str) -> bool {
    let mut parts = full_name.split('/');
    let ok = |p: Option<&str>| {
        p.is_some_and(|p| !p.is_empty() && p.len() <= 100 && p.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)))
    };
    ok(parts.next()) && ok(parts.next()) && parts.next().is_none()
}

/// Branch names Sesame accepts (no shell or ref tricks).
pub fn valid_branch(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('-')
        && !name.contains("..")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c))
}

impl Client {
    pub fn new(token: impl Into<String>) -> Result<Self> {
        Ok(Self {
            token: token.into(),
            http: reqwest::Client::builder()
                .user_agent(concat!("sesame/", env!("CARGO_PKG_VERSION")))
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
        })
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.http
            .get(format!("{API}{path}"))
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
    }

    async fn json<T: DeserializeOwned>(req: reqwest::RequestBuilder) -> Result<T> {
        let resp = req.send().await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            let msg = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
                .unwrap_or_else(|| body.chars().take(200).collect());
            bail!("GitHub {status}: {msg}");
        }
        serde_json::from_str(&body).context("unexpected GitHub response")
    }

    pub async fn user(&self) -> Result<User> {
        Self::json(self.get("/user")).await
    }

    /// Repositories the token can see, most recently pushed first.
    pub async fn repos(&self) -> Result<Vec<Repo>> {
        let mut out: Vec<Repo> = Vec::new();
        for page in 1..=10 {
            let batch: Vec<Repo> =
                Self::json(self.get(&format!("/user/repos?per_page=100&page={page}&sort=pushed&affiliation=owner,collaborator,organization_member"))).await?;
            let done = batch.len() < 100;
            out.extend(batch);
            if done {
                break;
            }
        }
        Ok(out)
    }

    pub async fn branches(&self, repo: &str) -> Result<Vec<Branch>> {
        if !valid_repo(repo) {
            bail!("invalid repository {repo}");
        }
        Self::json(self.get(&format!("/repos/{repo}/branches?per_page=100"))).await
    }

    pub async fn head(&self, repo: &str, branch: &str) -> Result<Commit> {
        if !valid_repo(repo) || !valid_branch(branch) {
            bail!("invalid repository or branch");
        }
        let v: serde_json::Value = Self::json(self.get(&format!("/repos/{repo}/commits/{branch}"))).await?;
        Ok(parse_commit(&v))
    }

    /// Adds a read-only deploy key unless the same key is already there.
    pub async fn ensure_deploy_key(&self, repo: &str, title: &str, public_key: &str) -> Result<()> {
        if !valid_repo(repo) {
            bail!("invalid repository {repo}");
        }
        let keys: Vec<DeployKey> = Self::json(self.get(&format!("/repos/{repo}/keys?per_page=100"))).await?;
        let wanted = key_body(public_key);
        if keys.iter().any(|k| key_body(&k.key) == wanted) {
            return Ok(());
        }
        let body = serde_json::json!({ "title": title, "key": public_key.trim(), "read_only": true });
        let req = self
            .http
            .post(format!("{API}/repos/{repo}/keys"))
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .json(&body);
        let _: serde_json::Value = Self::json(req).await?;
        Ok(())
    }
}

/// `ssh-ed25519 AAAA… comment` → `ssh-ed25519 AAAA…` for comparison.
fn key_body(k: &str) -> String {
    k.split_whitespace().take(2).collect::<Vec<_>>().join(" ")
}

fn parse_commit(v: &serde_json::Value) -> Commit {
    let s = |p: &str| v.pointer(p).and_then(|x| x.as_str()).unwrap_or("").to_string();
    Commit {
        sha: s("/sha"),
        message: s("/commit/message").lines().next().unwrap_or("").to_string(),
        author: s("/commit/author/name"),
        date: s("/commit/author/date"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_names() {
        assert!(valid_repo("rasul-chopurov/sesame"));
        assert!(!valid_repo("a/b/c"));
        assert!(!valid_repo("a;rm/b"));
        assert!(valid_branch("feature/glass-ui"));
        assert!(!valid_branch("--upload-pack=x"));
        assert!(!valid_branch("a..b"));
        assert!(!valid_branch("a b"));
    }

    #[test]
    fn parses_commit_and_compares_keys() {
        let v = serde_json::json!({"sha": "abc123", "commit": {"message": "Fix deploy\n\nbody", "author": {"name": "Rasul", "date": "2026-10-26T10:00:00Z"}}});
        let c = parse_commit(&v);
        assert_eq!(c.message, "Fix deploy");
        assert_eq!(c.author, "Rasul");
        assert_eq!(key_body("ssh-ed25519 AAAAC3Nz sesame@shop"), key_body("ssh-ed25519 AAAAC3Nz other"));
    }

    #[test]
    fn parses_repos() {
        let body = r#"[{"full_name":"rasul-chopurov/shop","private":true,"default_branch":"main","ssh_url":"git@github.com:rasul-chopurov/shop.git","pushed_at":"2026-10-25T10:00:00Z","description":null}]"#;
        let repos: Vec<Repo> = serde_json::from_str(body).unwrap();
        assert_eq!(repos[0].default_branch, "main");
        assert!(repos[0].private);
    }
}
