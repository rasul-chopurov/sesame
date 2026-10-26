//! Secrets API used across Sesame, backed by the encrypted [`Vault`] in the
//! local database. The vault must be unlocked once per process (the app asks
//! for the master password; the CLI reads `SESAME_MASTER_PASSWORD` or prompts).

use std::sync::RwLock;

use anyhow::{Result, anyhow};

use crate::ServerId;
use crate::vault::Vault;

static VAULT: RwLock<Option<Vault>> = RwLock::new(None);

/// Makes `vault` the process-wide unlocked vault.
pub fn install(vault: Vault) {
    *VAULT.write().unwrap() = Some(vault);
}

/// Forgets the key (locks the vault).
pub fn lock() {
    *VAULT.write().unwrap() = None;
}

pub fn is_unlocked() -> bool {
    VAULT.read().unwrap().is_some()
}

fn with<T>(f: impl FnOnce(&Vault) -> Result<T>) -> Result<T> {
    let guard = VAULT.read().unwrap();
    let v = guard.as_ref().ok_or_else(|| anyhow!("the Sesame vault is locked"))?;
    f(v)
}

/// Changes or sets the master password of the unlocked vault.
pub fn set_master_password(password: &str) -> Result<()> {
    let mut guard = VAULT.write().unwrap();
    let v = guard.as_mut().ok_or_else(|| anyhow!("the Sesame vault is locked"))?;
    v.set_password(password)
}

#[derive(Debug, Clone, Copy)]
pub enum SecretKind {
    Password,
    KeyPassphrase,
}

impl SecretKind {
    fn name(self) -> &'static str {
        match self {
            SecretKind::Password => "ssh-password",
            SecretKind::KeyPassphrase => "ssh-key-passphrase",
        }
    }
}

pub fn set(server: ServerId, kind: SecretKind, value: &str) -> Result<()> {
    with(|v| v.put(&server.to_string(), kind.name(), value))
}

pub fn get(server: ServerId, kind: SecretKind) -> Result<Option<String>> {
    with(|v| Ok(v.get(&server.to_string(), kind.name())?.map(|s| s.to_string())))
}

pub fn delete(server: ServerId, kind: SecretKind) -> Result<()> {
    with(|v| v.delete(&server.to_string(), Some(kind.name())))
}

/// A generated credential for a server (database password, S3 key, …).
pub fn set_named(server: ServerId, name: &str, value: &str) -> Result<()> {
    with(|v| v.put(&server.to_string(), name, value))
}

pub fn get_named(server: ServerId, name: &str) -> Result<Option<String>> {
    with(|v| Ok(v.get(&server.to_string(), name)?.map(|s| s.to_string())))
}

/// Everything stored for a server (when it is removed).
pub fn delete_server(server: ServerId) -> Result<()> {
    with(|v| v.delete(&server.to_string(), None))
}

/// Account-wide credentials: GitHub and Cloudflare tokens.
pub fn set_global(name: &str, value: &str) -> Result<()> {
    with(|v| v.put("global", name, value))
}

pub fn get_global(name: &str) -> Result<Option<String>> {
    with(|v| Ok(v.get("global", name)?.map(|s| s.to_string())))
}

pub fn delete_global(name: &str) -> Result<()> {
    with(|v| v.delete("global", Some(name)))
}
