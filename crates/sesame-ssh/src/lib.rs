//! SSH transport: connect with ssh-agent, a key file or a password, verify
//! the host key against `~/.ssh/known_hosts`, run commands.
//!
//! Everything above this crate talks to servers through [`Exec`], so
//! discovery and actions can be tested against recorded command output.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use russh::client::{self, AuthResult, Handle};
use russh::keys::agent::client::AgentClient;
use russh::keys::{PrivateKeyWithHashAlg, PublicKeyOrCertificate, known_hosts};
use russh::{ChannelMsg, keys};
use sesame_core::secrets::{self, SecretKind};
use sesame_core::{AuthMethod, ServerConfig};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<u32>,
}

impl CommandOutput {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }
}

/// Runs a shell command on a server.
pub trait Exec: Send + Sync {
    fn exec(&self, command: &str) -> impl Future<Output = Result<CommandOutput>> + Send;
}

/// What happened to the server's host key on connect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyStatus {
    /// Matched `~/.ssh/known_hosts`.
    Known,
    /// Not seen before: trusted on first use and written to `known_hosts`.
    Learned { fingerprint: String },
}

struct Verifier {
    host: String,
    port: u16,
    status: Arc<Mutex<Option<HostKeyStatus>>>,
}

impl client::Handler for Verifier {
    type Error = anyhow::Error;

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool> {
        let PublicKeyOrCertificate::PublicKey { key, .. } = key else {
            bail!("host certificates are not supported yet");
        };
        let known = known_hosts::known_host_keys(&self.host, self.port).unwrap_or_default();
        if known.is_empty() {
            known_hosts::learn_known_hosts(&self.host, self.port, key)
                .context("could not write ~/.ssh/known_hosts")?;
            let fingerprint = key.fingerprint(Default::default()).to_string();
            *self.status.lock().unwrap() = Some(HostKeyStatus::Learned { fingerprint });
            return Ok(true);
        }
        match known_hosts::check_known_hosts(&self.host, self.port, key) {
            Ok(true) => {
                *self.status.lock().unwrap() = Some(HostKeyStatus::Known);
                Ok(true)
            }
            Ok(false) | Err(keys::Error::KeyChanged { .. }) => bail!(
                "HOST KEY CHANGED for {}:{} — the server's key does not match ~/.ssh/known_hosts. \
                 Refusing to connect (possible man-in-the-middle).",
                self.host,
                self.port
            ),
            Err(e) => Err(e.into()),
        }
    }
}

pub struct Session {
    handle: Handle<Verifier>,
    pub host_key: HostKeyStatus,
    timeout: Duration,
}

impl Session {
    pub async fn connect(server: &ServerConfig) -> Result<Self> {
        let config = Arc::new(client::Config {
            inactivity_timeout: Some(Duration::from_secs(300)),
            keepalive_interval: Some(Duration::from_secs(30)),
            ..Default::default()
        });
        let status = Arc::new(Mutex::new(None));
        let verifier = Verifier {
            host: server.host.clone(),
            port: server.port,
            status: status.clone(),
        };
        let mut handle = tokio::time::timeout(
            Duration::from_secs(15),
            client::connect(config, (server.host.as_str(), server.port), verifier),
        )
        .await
        .map_err(|_| anyhow!("timed out connecting to {}", server.address()))??;

        authenticate(&mut handle, server).await?;
        let host_key = status.lock().unwrap().clone().unwrap_or(HostKeyStatus::Known);
        Ok(Self { handle, host_key, timeout: Duration::from_secs(120) })
    }

    pub async fn close(self) -> Result<()> {
        self.handle
            .disconnect(russh::Disconnect::ByApplication, "", "en")
            .await?;
        Ok(())
    }

    async fn run(&self, command: &str) -> Result<CommandOutput> {
        let mut channel = self.handle.channel_open_session().await?;
        channel.exec(true, command).await?;
        let mut out = CommandOutput::default();
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => stdout.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, .. } => stderr.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => out.exit_code = Some(exit_status),
                _ => {}
            }
        }
        out.stdout = String::from_utf8_lossy(&stdout).into_owned();
        out.stderr = String::from_utf8_lossy(&stderr).into_owned();
        Ok(out)
    }
}

impl Exec for Session {
    async fn exec(&self, command: &str) -> Result<CommandOutput> {
        tokio::time::timeout(self.timeout, self.run(command))
            .await
            .map_err(|_| anyhow!("command timed out after {:?}", self.timeout))?
    }
}

async fn authenticate(handle: &mut Handle<Verifier>, server: &ServerConfig) -> Result<()> {
    let user = server.user.clone();
    let ok = match &server.auth {
        AuthMethod::Password => {
            let password = secrets::get(server.id, SecretKind::Password)?
                .context("no password saved for this server")?;
            handle.authenticate_password(user, password).await?.success()
        }
        AuthMethod::KeyFile { path } => {
            let path = expand_home(path);
            let passphrase = secrets::get(server.id, SecretKind::KeyPassphrase)?;
            let key = keys::load_secret_key(&path, passphrase.as_deref())
                .with_context(|| format!("could not load key {path}"))?;
            let hash = handle.best_supported_rsa_hash().await?.flatten();
            handle
                .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                .await?
                .success()
        }
        AuthMethod::Agent => agent_auth(handle, &user).await?,
    };
    if !ok {
        bail!("authentication failed for {}", server.address());
    }
    Ok(())
}

/// Tries each agent identity, then falls back to the usual key files in
/// `~/.ssh` without a passphrase (what `ssh` itself does).
async fn agent_auth(handle: &mut Handle<Verifier>, user: &str) -> Result<bool> {
    let hash = handle.best_supported_rsa_hash().await?.flatten();
    if let Ok(mut agent) = AgentClient::connect_env().await {
        for identity in agent.request_identities().await.unwrap_or_default() {
            let key = identity.public_key().into_owned();
            if let Ok(AuthResult::Success) = handle
                .authenticate_publickey_with(user, key, hash, &mut agent)
                .await
            {
                return Ok(true);
            }
        }
    }
    let Some(home) = dirs::home_dir() else { return Ok(false) };
    for name in ["id_ed25519", "id_ecdsa", "id_rsa"] {
        let path = home.join(".ssh").join(name);
        let Ok(key) = keys::load_secret_key(&path, None) else { continue };
        let result = handle
            .authenticate_publickey(user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
            .await?;
        if result.success() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest).to_string_lossy().into_owned(),
        _ => path.to_string(),
    }
}
