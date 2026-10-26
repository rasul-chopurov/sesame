//! Encrypted secrets in the local SQLite database.
//!
//! - The master password is never stored. Argon2id (64 MiB, 3 passes) turns
//!   it and a random salt into a 256-bit key.
//! - Each secret is sealed with XChaCha20-Poly1305 under a fresh random
//!   192-bit nonce; its scope and name are bound in as associated data, so a
//!   ciphertext cannot be moved to another slot without failing to open.
//! - A known value sealed at creation verifies the password on unlock.
//! - The key lives only in memory and is wiped on drop.
//!
//! Without a master password the key is a random file next to the database
//! (`vault.key`, mode 0600): it protects a copied database, not a stolen disk.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

use crate::SqliteStore;

const VERIFIER: &[u8] = b"sesame-vault-v1";
const M_COST_KIB: u32 = 64 * 1024;
const T_COST: u32 = 3;
const P_COST: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultMode {
    /// Not set up yet.
    New,
    /// Key derived from a master password.
    Password,
    /// Key in `vault.key` next to the database.
    KeyFile,
}

pub struct Vault {
    store: Arc<SqliteStore>,
    key: Zeroizing<[u8; 32]>,
}

fn random<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|e| anyhow!("system randomness: {e}"))?;
    Ok(b)
}

fn derive(password: &str, salt: &[u8], m: u32, t: u32, p: u32) -> Result<Zeroizing<[u8; 32]>> {
    let params = argon2::Params::new(m, t, p, Some(32)).map_err(|e| anyhow!("{e}"))?;
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(password.as_bytes(), salt, key.as_mut())
        .map_err(|e| anyhow!("{e}"))?;
    Ok(key)
}

fn seal(key: &[u8; 32], aad: &[u8], plaintext: &[u8]) -> Result<([u8; 24], Vec<u8>)> {
    let cipher = XChaCha20Poly1305::new(key.into());
    let nonce: [u8; 24] = random()?;
    let ct = cipher
        .encrypt(&XNonce::from(nonce), Payload { msg: plaintext, aad })
        .map_err(|_| anyhow!("encryption failed"))?;
    Ok((nonce, ct))
}

fn open(key: &[u8; 32], aad: &[u8], nonce: &[u8], ct: &[u8]) -> Result<Zeroizing<Vec<u8>>> {
    let nonce: [u8; 24] = nonce.try_into().map_err(|_| anyhow!("corrupt vault entry"))?;
    let cipher = XChaCha20Poly1305::new(key.into());
    let pt = cipher
        .decrypt(&XNonce::from(nonce), Payload { msg: ct, aad })
        .map_err(|_| anyhow!("cannot decrypt (wrong password or tampered data)"))?;
    Ok(Zeroizing::new(pt))
}

fn aad(scope: &str, name: &str) -> Vec<u8> {
    format!("sesame\0{scope}\0{name}").into_bytes()
}

fn u32_meta(store: &SqliteStore, key: &str) -> Result<Option<u32>> {
    Ok(store
        .vault_meta(key)?
        .and_then(|b| <[u8; 4]>::try_from(b.as_slice()).ok())
        .map(u32::from_le_bytes))
}

impl Vault {
    pub fn mode(store: &SqliteStore) -> Result<VaultMode> {
        Ok(match store.vault_meta("mode")?.as_deref() {
            None => VaultMode::New,
            Some(b"password") => VaultMode::Password,
            Some(b"keyfile") => VaultMode::KeyFile,
            Some(_) => bail!("unknown vault mode"),
        })
    }

    pub fn key_file_path() -> Result<PathBuf> {
        Ok(SqliteStore::data_dir()?.join("vault.key"))
    }

    fn write_verifier(store: &SqliteStore, key: &[u8; 32]) -> Result<()> {
        let (nonce, ct) = seal(key, b"verifier", VERIFIER)?;
        store.set_vault_meta("verifier_nonce", &nonce)?;
        store.set_vault_meta("verifier", &ct)
    }

    fn check_verifier(store: &SqliteStore, key: &[u8; 32]) -> Result<()> {
        let nonce = store.vault_meta("verifier_nonce")?.context("vault is not set up")?;
        let ct = store.vault_meta("verifier")?.context("vault is not set up")?;
        let pt = open(key, b"verifier", &nonce, &ct).map_err(|_| anyhow!("wrong master password"))?;
        if pt.as_slice() != VERIFIER {
            bail!("wrong master password");
        }
        Ok(())
    }

    /// First run with a master password.
    pub fn create_with_password(store: Arc<SqliteStore>, password: &str) -> Result<Self> {
        Self::check_strength(password)?;
        if Self::mode(&store)? != VaultMode::New {
            bail!("the vault already exists");
        }
        let salt: [u8; 16] = random()?;
        let key = derive(password, &salt, M_COST_KIB, T_COST, P_COST)?;
        store.set_vault_meta("salt", &salt)?;
        store.set_vault_meta("m_cost", &M_COST_KIB.to_le_bytes())?;
        store.set_vault_meta("t_cost", &T_COST.to_le_bytes())?;
        store.set_vault_meta("p_cost", &P_COST.to_le_bytes())?;
        Self::write_verifier(&store, &key)?;
        store.set_vault_meta("mode", b"password")?;
        Ok(Self { store, key })
    }

    pub fn unlock_with_password(store: Arc<SqliteStore>, password: &str) -> Result<Self> {
        if Self::mode(&store)? != VaultMode::Password {
            bail!("the vault does not use a master password");
        }
        let salt = store.vault_meta("salt")?.context("vault salt missing")?;
        let m = u32_meta(&store, "m_cost")?.unwrap_or(M_COST_KIB);
        let t = u32_meta(&store, "t_cost")?.unwrap_or(T_COST);
        let p = u32_meta(&store, "p_cost")?.unwrap_or(P_COST);
        let key = derive(password, &salt, m, t, p)?;
        Self::check_verifier(&store, &key)?;
        Ok(Self { store, key })
    }

    /// First run without a master password: the key goes to a 0600 file.
    pub fn create_with_key_file(store: Arc<SqliteStore>, path: &Path) -> Result<Self> {
        if Self::mode(&store)? != VaultMode::New {
            bail!("the vault already exists");
        }
        let key = Zeroizing::new(random::<32>()?);
        write_private(path, key.as_ref())?;
        Self::write_verifier(&store, &key)?;
        store.set_vault_meta("mode", b"keyfile")?;
        Ok(Self { store, key })
    }

    pub fn unlock_with_key_file(store: Arc<SqliteStore>, path: &Path) -> Result<Self> {
        let bytes = Zeroizing::new(std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?);
        let key: Zeroizing<[u8; 32]> =
            Zeroizing::new(bytes.as_slice().try_into().map_err(|_| anyhow!("corrupt key file"))?);
        Self::check_verifier(&store, &key)?;
        Ok(Self { store, key })
    }

    /// At least 10 characters; the vault guards SSH and database credentials.
    pub fn check_strength(password: &str) -> Result<()> {
        if password.chars().count() < 10 {
            bail!("the master password needs at least 10 characters");
        }
        Ok(())
    }

    pub fn put(&self, scope: &str, name: &str, value: &str) -> Result<()> {
        let (nonce, ct) = seal(&self.key, &aad(scope, name), value.as_bytes())?;
        self.store.vault_put(scope, name, &nonce, &ct)
    }

    pub fn get(&self, scope: &str, name: &str) -> Result<Option<Zeroizing<String>>> {
        let Some((nonce, ct)) = self.store.vault_get(scope, name)? else { return Ok(None) };
        let pt = open(&self.key, &aad(scope, name), &nonce, &ct)?;
        Ok(Some(Zeroizing::new(String::from_utf8(pt.to_vec()).context("corrupt secret")?)))
    }

    pub fn delete(&self, scope: &str, name: Option<&str>) -> Result<()> {
        self.store.vault_delete(scope, name)
    }

    /// Switches to a (new) master password, re-sealing every secret.
    pub fn set_password(&mut self, password: &str) -> Result<()> {
        Self::check_strength(password)?;
        let salt: [u8; 16] = random()?;
        let new_key = derive(password, &salt, M_COST_KIB, T_COST, P_COST)?;
        let mut resealed = Vec::new();
        for (scope, name, nonce, ct) in self.store.vault_all()? {
            let pt = open(&self.key, &aad(&scope, &name), &nonce, &ct)?;
            let (n, c) = seal(&new_key, &aad(&scope, &name), &pt)?;
            resealed.push((scope, name, n, c));
        }
        for (scope, name, n, c) in resealed {
            self.store.vault_put(&scope, &name, &n, &c)?;
        }
        self.store.set_vault_meta("salt", &salt)?;
        self.store.set_vault_meta("m_cost", &M_COST_KIB.to_le_bytes())?;
        self.store.set_vault_meta("t_cost", &T_COST.to_le_bytes())?;
        self.store.set_vault_meta("p_cost", &P_COST.to_le_bytes())?;
        Self::write_verifier(&self.store, &new_key)?;
        self.store.set_vault_meta("mode", b"password")?;
        self.key = new_key;
        if let Ok(path) = Self::key_file_path() {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
        f.write_all(bytes)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Arc<SqliteStore> {
        Arc::new(SqliteStore::in_memory().unwrap())
    }

    #[test]
    fn password_vault_round_trip() {
        let s = store();
        assert_eq!(Vault::mode(&s).unwrap(), VaultMode::New);
        let v = Vault::create_with_password(s.clone(), "correct horse battery").unwrap();
        v.put("srv-1", "DB_PASSWORD", "hunter2").unwrap();
        drop(v);

        assert!(Vault::unlock_with_password(s.clone(), "wrong password!!").is_err());
        let v = Vault::unlock_with_password(s.clone(), "correct horse battery").unwrap();
        assert_eq!(v.get("srv-1", "DB_PASSWORD").unwrap().unwrap().as_str(), "hunter2");
        assert!(v.get("srv-1", "missing").unwrap().is_none());
    }

    #[test]
    fn nothing_readable_is_stored() {
        let s = store();
        let v = Vault::create_with_password(s.clone(), "correct horse battery").unwrap();
        v.put("srv-1", "TOKEN", "ghp_supersecretvalue").unwrap();
        for (_, _, nonce, ct) in s.vault_all().unwrap() {
            assert_eq!(nonce.len(), 24);
            assert!(!ct.windows(6).any(|w| w == b"ghp_su"));
        }
        // Same value twice → different ciphertexts (fresh nonces).
        v.put("srv-2", "TOKEN", "ghp_supersecretvalue").unwrap();
        let all = s.vault_all().unwrap();
        assert_ne!(all[0].3, all[1].3);
    }

    #[test]
    fn ciphertext_cannot_move_between_slots() {
        let s = store();
        let v = Vault::create_with_password(s.clone(), "correct horse battery").unwrap();
        v.put("srv-1", "A", "one").unwrap();
        let (n, c) = s.vault_get("srv-1", "A").unwrap().unwrap();
        s.vault_put("srv-2", "A", &n, &c).unwrap();
        assert!(v.get("srv-2", "A").is_err());
    }

    #[test]
    fn key_file_mode_and_upgrade_to_password() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.key");
        let s = store();
        let v = Vault::create_with_key_file(s.clone(), &path).unwrap();
        v.put("global", "CF_TOKEN", "cf-123").unwrap();
        drop(v);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let mut v = Vault::unlock_with_key_file(s.clone(), &path).unwrap();
        v.set_password("a much better password").unwrap();
        assert_eq!(Vault::mode(&s).unwrap(), VaultMode::Password);
        let v = Vault::unlock_with_password(s.clone(), "a much better password").unwrap();
        assert_eq!(v.get("global", "CF_TOKEN").unwrap().unwrap().as_str(), "cf-123");
    }

    #[test]
    fn weak_passwords_are_refused() {
        assert!(Vault::create_with_password(store(), "short").is_err());
    }
}
