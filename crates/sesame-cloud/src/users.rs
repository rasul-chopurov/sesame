//! People who sign in to Sesame Cloud, their roles and sessions.
//!
//! Passwords are Argon2id hashes; sessions and API tokens are random and only
//! their SHA-256 is stored.

use std::path::Path;
use std::sync::Mutex;

use anyhow::{Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Viewer,
    Developer,
    Admin,
    Owner,
}

impl Role {
    pub fn parse(s: &str) -> Option<Role> {
        Some(match s {
            "viewer" => Role::Viewer,
            "developer" => Role::Developer,
            "admin" => Role::Admin,
            "owner" => Role::Owner,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Role::Viewer => "viewer",
            Role::Developer => "developer",
            Role::Admin => "admin",
            Role::Owner => "owner",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub name: String,
    pub role: Role,
    /// A token user is an agent (MCP/REST client), not a person.
    pub agent: bool,
}

pub struct Users {
    conn: Mutex<Connection>,
}

fn sha(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

pub fn random_token(prefix: &str) -> Result<String> {
    let mut b = [0u8; 24];
    getrandom::fill(&mut b).map_err(|e| anyhow!("{e}"))?;
    Ok(format!("{prefix}{}", hex::encode(b)))
}

const M_COST: u32 = 19 * 1024;
const T_COST: u32 = 2;

fn hash_password(password: &str) -> Result<String> {
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|e| anyhow!("{e}"))?;
    let params = argon2::Params::new(M_COST, T_COST, 1, Some(32)).map_err(|e| anyhow!("{e}"))?;
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut out = [0u8; 32];
    argon.hash_password_into(password.as_bytes(), &salt, &mut out).map_err(|e| anyhow!("{e}"))?;
    Ok(format!("argon2id${M_COST}${T_COST}${}${}", hex::encode(salt), hex::encode(out)))
}

fn verify_password(password: &str, stored: &str) -> bool {
    let parts: Vec<&str> = stored.split('$').collect();
    let [_, m, t, salt, hash] = parts.as_slice() else { return false };
    let (Ok(m), Ok(t), Ok(salt), Ok(hash)) = (m.parse(), t.parse(), hex::decode(salt), hex::decode(hash)) else { return false };
    let Ok(params) = argon2::Params::new(m, t, 1, Some(32)) else { return false };
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut out = [0u8; 32];
    if argon.hash_password_into(password.as_bytes(), &salt, &mut out).is_err() {
        return false;
    }
    // Constant-time comparison.
    out.len() == hash.len() && out.iter().zip(&hash).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

impl Users {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY,
                email TEXT NOT NULL UNIQUE,
                name TEXT NOT NULL,
                role TEXT NOT NULL,
                password TEXT,
                agent INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS sessions (
                token_sha256 TEXT PRIMARY KEY,
                user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                created_at TEXT NOT NULL
             );",
        )?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn is_empty(&self) -> Result<bool> {
        let n: i64 = self.conn.lock().unwrap().query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0))?;
        Ok(n == 0)
    }

    fn row(r: &rusqlite::Row) -> rusqlite::Result<User> {
        Ok(User {
            id: r.get(0)?,
            email: r.get(1)?,
            name: r.get(2)?,
            role: Role::parse(&r.get::<_, String>(3)?).unwrap_or(Role::Viewer),
            agent: r.get::<_, i64>(4)? != 0,
        })
    }

    pub fn list(&self) -> Result<Vec<User>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id, email, name, role, agent FROM users ORDER BY id")?;
        Ok(stmt.query_map([], Self::row)?.collect::<rusqlite::Result<_>>()?)
    }

    /// Adds a person with a password (at least 10 characters).
    pub fn create(&self, email: &str, name: &str, role: Role, password: &str) -> Result<User> {
        let email = email.trim().to_lowercase();
        if !email.contains('@') || email.len() > 200 {
            bail!("a valid email is needed");
        }
        if password.chars().count() < 10 {
            bail!("passwords need at least 10 characters");
        }
        let hash = hash_password(password)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO users (email, name, role, password, agent) VALUES (?1, ?2, ?3, ?4, 0)",
            params![email, name.trim(), role.as_str(), hash],
        )
        .map_err(|e| if e.to_string().contains("UNIQUE") { anyhow!("{email} already has an account") } else { e.into() })?;
        let id = conn.last_insert_rowid();
        Ok(User { id, email, name: name.trim().into(), role, agent: false })
    }

    /// Adds an agent (MCP/REST client) and returns its token, shown once.
    pub fn create_agent(&self, name: &str, role: Role) -> Result<(User, String)> {
        let token = random_token("sesc_")?;
        let email = format!("agent-{}@agents.local", &sha(&token)[..12]);
        let conn = self.conn.lock().unwrap();
        conn.execute("INSERT INTO users (email, name, role, password, agent) VALUES (?1, ?2, ?3, NULL, 1)", params![email, name.trim(), role.as_str()])?;
        let id = conn.last_insert_rowid();
        conn.execute("INSERT INTO sessions (token_sha256, user_id, created_at) VALUES (?1, ?2, ?3)", params![sha(&token), id, now()])?;
        Ok((User { id, email, name: name.trim().into(), role, agent: true }, token))
    }

    /// Checks email and password; returns a new session token.
    pub fn login(&self, email: &str, password: &str) -> Result<Option<(User, String)>> {
        let email = email.trim().to_lowercase();
        let found: Option<(User, Option<String>)> = {
            let conn = self.conn.lock().unwrap();
            conn.query_row("SELECT id, email, name, role, agent, password FROM users WHERE email = ?1 AND agent = 0", [&email], |r| {
                Ok((Self::row(r)?, r.get(5)?))
            })
            .optional()?
        };
        let Some((user, Some(hash))) = found else {
            // Spend the same time as a real check so missing accounts are not obvious.
            let _ = verify_password(password, "argon2id$19456$2$00000000000000000000000000000000$00");
            return Ok(None);
        };
        if !verify_password(password, &hash) {
            return Ok(None);
        }
        let token = random_token("sess_")?;
        self.conn
            .lock()
            .unwrap()
            .execute("INSERT INTO sessions (token_sha256, user_id, created_at) VALUES (?1, ?2, ?3)", params![sha(&token), user.id, now()])?;
        Ok(Some((user, token)))
    }

    pub fn by_token(&self, token: &str) -> Result<Option<User>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn
            .query_row(
                "SELECT u.id, u.email, u.name, u.role, u.agent FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.token_sha256 = ?1",
                [sha(token)],
                Self::row,
            )
            .optional()?)
    }

    pub fn logout(&self, token: &str) -> Result<()> {
        self.conn.lock().unwrap().execute("DELETE FROM sessions WHERE token_sha256 = ?1", [sha(token)])?;
        Ok(())
    }

    pub fn set_role(&self, id: i64, role: Role) -> Result<()> {
        self.conn.lock().unwrap().execute("UPDATE users SET role = ?2 WHERE id = ?1", params![id, role.as_str()])?;
        Ok(())
    }

    /// Removes a person or agent and ends their sessions.
    pub fn delete(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM sessions WHERE user_id = ?1", [id])?;
        conn.execute("DELETE FROM users WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn get(&self, id: i64) -> Result<Option<User>> {
        let conn = self.conn.lock().unwrap();
        Ok(conn.query_row("SELECT id, email, name, role, agent FROM users WHERE id = ?1", [id], Self::row).optional()?)
    }
}

fn now() -> String {
    time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accounts_sessions_and_agents() {
        let u = Users::in_memory().unwrap();
        assert!(u.is_empty().unwrap());
        assert!(u.create("a@b.c", "Anna", Role::Owner, "short").is_err());
        let anna = u.create("Anna@Example.com", "Anna", Role::Owner, "correct horse battery").unwrap();
        assert_eq!(anna.email, "anna@example.com");
        assert!(u.create("anna@example.com", "x", Role::Viewer, "another password").is_err(), "unique email");
        assert!(u.login("anna@example.com", "wrong password!").unwrap().is_none());
        let (_, token) = u.login("ANNA@example.com", "correct horse battery").unwrap().unwrap();
        assert_eq!(u.by_token(&token).unwrap().unwrap().id, anna.id);
        u.logout(&token).unwrap();
        assert!(u.by_token(&token).unwrap().is_none());
        let (agent, t) = u.create_agent("claude", Role::Developer).unwrap();
        assert!(agent.agent);
        assert_eq!(u.by_token(&t).unwrap().unwrap().role, Role::Developer);
        assert!(u.login(&agent.email, "").unwrap().is_none(), "agents cannot log in with a password");
        u.delete(agent.id).unwrap();
        assert!(u.by_token(&t).unwrap().is_none());
    }
}
