use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub type ServerId = uuid::Uuid;

/// A server the user added. Secrets (passwords, key passphrases) never live
/// here; they are in the OS keychain under the server id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServerConfig {
    pub id: ServerId,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthMethod,
}

impl ServerConfig {
    pub fn new(name: impl Into<String>, host: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            name: name.into(),
            host: host.into(),
            port: 22,
            user: user.into(),
            auth: AuthMethod::Agent,
        }
    }

    pub fn address(&self) -> String {
        format!("{}@{}:{}", self.user, self.host, self.port)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuthMethod {
    /// Keys from the running ssh-agent (`SSH_AUTH_SOCK`).
    Agent,
    /// A private key file; its passphrase, if any, is in the keychain.
    KeyFile { path: String },
    /// Password from the keychain.
    Password,
}
