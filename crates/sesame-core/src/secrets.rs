//! Secrets live in the OS keychain (macOS Keychain, Windows Credential
//! Manager), keyed by server id and purpose. Nothing secret goes to SQLite.

use anyhow::Result;

use crate::ServerId;

const SERVICE: &str = "dev.sesame.app";

#[derive(Debug, Clone, Copy)]
pub enum SecretKind {
    Password,
    KeyPassphrase,
}

impl SecretKind {
    fn suffix(self) -> &'static str {
        match self {
            SecretKind::Password => "password",
            SecretKind::KeyPassphrase => "key-passphrase",
        }
    }
}

fn entry(server: ServerId, kind: SecretKind) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(SERVICE, &format!("{server}:{}", kind.suffix()))?)
}

pub fn set(server: ServerId, kind: SecretKind, value: &str) -> Result<()> {
    entry(server, kind)?.set_password(value)?;
    Ok(())
}

pub fn get(server: ServerId, kind: SecretKind) -> Result<Option<String>> {
    match entry(server, kind)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn delete(server: ServerId, kind: SecretKind) -> Result<()> {
    match entry(server, kind)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn named_entry(server: ServerId, name: &str) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(SERVICE, &format!("{server}:value:{name}"))?)
}

/// Stores a generated credential (a database password, an S3 key, …).
pub fn set_named(server: ServerId, name: &str, value: &str) -> Result<()> {
    named_entry(server, name)?.set_password(value)?;
    Ok(())
}

pub fn get_named(server: ServerId, name: &str) -> Result<Option<String>> {
    match named_entry(server, name)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
