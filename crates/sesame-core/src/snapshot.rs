//! What discovery found on a server. Plain data, serialisable, so it is
//! cached in SQLite, shown in the UI and returned to agents as-is.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ServerSnapshot {
    /// RFC 3339 time the snapshot was taken.
    pub taken_at: String,
    pub os: OsInfo,
    pub services: Vec<SystemService>,
    pub supervisor: Vec<SupervisorProgram>,
    pub ports: Vec<ListeningPort>,
    pub web_servers: Vec<WebServer>,
    pub sites: Vec<Site>,
    pub databases: Vec<DatabaseEngine>,
    pub mail: MailInfo,
    pub php: Vec<PhpInstall>,
    pub cron: Vec<CronEntry>,
    pub certificates: Vec<Certificate>,
    pub firewall: FirewallInfo,
    pub containers: Vec<Container>,
    /// Detectors that could not run (missing tool, no permission). Shown so
    /// the user knows the picture is partial.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OsInfo {
    pub hostname: String,
    pub distro: String,
    pub kernel: String,
    pub arch: String,
    pub cpus: u32,
    pub mem_total_bytes: u64,
    pub mem_available_bytes: u64,
    pub disk_total_bytes: u64,
    pub disk_used_bytes: u64,
    pub uptime_seconds: u64,
    pub load: [f32; 3],
    /// Whether discovery ran as root or with passwordless sudo.
    pub privileged: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SystemService {
    pub unit: String,
    pub active: String,
    pub sub: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SupervisorProgram {
    pub name: String,
    pub state: String,
    pub pid: Option<u32>,
    pub uptime: Option<String>,
    pub command: Option<String>,
    pub directory: Option<String>,
    pub user: Option<String>,
    pub role: ProcessRole,
}

/// What a long-running process does for a Laravel app, guessed from its command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProcessRole {
    Octane,
    Queue,
    Horizon,
    Reverb,
    Scheduler,
    Node,
    #[default]
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ListeningPort {
    pub address: String,
    pub port: u16,
    pub process: String,
    /// True when bound to all interfaces, i.e. reachable unless a firewall stops it.
    pub public: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WebServer {
    pub kind: WebServerKind,
    pub version: Option<String>,
    pub config_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WebServerKind {
    Caddy,
    FrankenPhp,
    Nginx,
    Apache,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Site {
    pub domains: Vec<String>,
    /// Document root as configured (often `<app>/public`).
    pub root: Option<String>,
    /// Application directory (root without a trailing `/public`).
    pub app_path: Option<String>,
    pub served_by: Option<WebServerKind>,
    /// Upstreams the site proxies to, e.g. `127.0.0.1:8000`.
    pub upstreams: Vec<String>,
    pub php: bool,
    pub tls: Option<String>,
    pub framework: Framework,
    pub framework_version: Option<String>,
    pub env: Option<AppEnv>,
    /// Supervisor programs running from this app's directory.
    pub processes: Vec<String>,
    /// The checkout in the app directory, when it is a git repository.
    #[serde(default)]
    pub git: Option<GitInfo>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GitInfo {
    pub sha: String,
    pub branch: Option<String>,
    pub remote: Option<String>,
    /// `owner/name` when the remote is on GitHub.
    pub github_repo: Option<String>,
}

impl GitInfo {
    /// `git@github.com:o/r.git` or `https://github.com/o/r(.git)` → `o/r`.
    pub fn github_repo_of(remote: &str) -> Option<String> {
        let rest = remote
            .strip_prefix("git@github.com:")
            .or_else(|| remote.strip_prefix("https://github.com/"))
            .or_else(|| remote.strip_prefix("ssh://git@github.com/"))?;
        let rest = rest.trim_end_matches('/').trim_end_matches(".git");
        (rest.split('/').count() == 2).then(|| rest.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Framework {
    Laravel,
    WordPress,
    Node,
    Static,
    #[default]
    Unknown,
}

/// The non-secret part of an app's `.env`. Passwords and keys are never read
/// during discovery.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AppEnv {
    pub app_env: Option<String>,
    pub app_url: Option<String>,
    pub db_connection: Option<String>,
    pub db_host: Option<String>,
    pub db_port: Option<String>,
    pub db_database: Option<String>,
    pub db_username: Option<String>,
    pub cache_store: Option<String>,
    pub queue_connection: Option<String>,
    pub session_driver: Option<String>,
    pub redis_host: Option<String>,
    pub mail_mailer: Option<String>,
    pub mail_host: Option<String>,
    pub broadcast_connection: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DatabaseEngine {
    pub kind: DatabaseKind,
    pub version: Option<String>,
    pub listen: Option<String>,
    /// Databases we could list without asking for credentials (root socket auth).
    pub databases: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseKind {
    MySql,
    MariaDb,
    Postgres,
    Redis,
    MongoDb,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MailInfo {
    /// Mail server software found running (postfix, exim4, dovecot, stalwart, …).
    pub servers: Vec<String>,
    pub hostname: Option<String>,
    pub domains: Vec<String>,
    /// When no local mail server runs: the mailers apps use (`smtp via smtp.resend.com`).
    pub external: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PhpInstall {
    pub version: String,
    pub cli: bool,
    pub fpm_running: bool,
    pub fpm_pools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CronEntry {
    pub user: String,
    pub schedule: String,
    pub command: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Certificate {
    pub path: String,
    pub subject: Option<String>,
    pub issuer: Option<String>,
    pub names: Vec<String>,
    /// RFC 3339 expiry.
    pub not_after: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FirewallInfo {
    pub ufw: Option<String>,
    pub fail2ban_jails: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Container {
    pub name: String,
    pub image: String,
    pub status: String,
    pub ports: String,
}
