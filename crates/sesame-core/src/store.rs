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
}

pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    /// Opens `state.db` in `$SESAME_DATA_DIR`, else the platform data
    /// directory (`~/Library/Application Support/dev.sesame.Sesame` on macOS).
    pub fn open_default() -> Result<Self> {
        let dir = match std::env::var_os("SESAME_DATA_DIR") {
            Some(d) => std::path::PathBuf::from(d),
            None => directories::ProjectDirs::from("dev", "sesame", "Sesame")
                .context("no home directory")?
                .data_dir()
                .to_path_buf(),
        };
        std::fs::create_dir_all(&dir)?;
        Self::open(&dir.join("state.db"))
    }

    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::migrate(&conn)?;
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
