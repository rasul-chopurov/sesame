use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use sesame_core::{AuthMethod, ServerConfig, SqliteStore, Store};

#[derive(Parser)]
#[command(name = "sesame", version, about = "Open your servers. Run without arguments for the app.")]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum DoAction {
    /// Install a service: redis, postgresql, mariadb, mongodb, elasticsearch, rustfs.
    Install { service: String },
    /// Uninstall a service (keeps data unless --delete-data).
    Uninstall {
        service: String,
        #[arg(long)]
        delete_data: bool,
    },
    /// Create a database and its own user: engine is mysql, mariadb, postgres or mongodb.
    DbCreate { engine: String, name: String, user: Option<String> },
    /// Back up and drop a database.
    DbDrop { engine: String, name: String, user: Option<String> },
    /// systemd or supervisor; action start|stop|restart|enable|disable.
    Service { kind: String, name: String, action: String },
    /// Laravel maintenance mode for an app directory: on|off.
    Maintenance { path: String, state: String },
    /// Install a certificate + private key (PEM files) as /etc/ssl/sesame/<name>/.
    Cert {
        name: String,
        #[arg(long)]
        cert: String,
        #[arg(long)]
        key: String,
    },
    /// Create an isolated site: own Linux user, PHP pool or Octane, web-server file, database, cron.
    NewSite {
        domain: String,
        /// laravel or static
        #[arg(long, default_value = "laravel")]
        kind: String,
        /// fpm or octane
        #[arg(long, default_value = "fpm")]
        runtime: String,
        /// Public GitHub repository (owner/name) to deploy instead of a skeleton.
        #[arg(long)]
        repo: Option<String>,
        #[arg(long, default_value = "main")]
        branch: String,
        /// Web root inside the app (public, website, .)
        #[arg(long)]
        root: Option<String>,
        /// mysql, mariadb or postgres
        #[arg(long)]
        db: Option<String>,
        #[arg(long)]
        alias: Vec<String>,
        #[arg(long)]
        no_scheduler: bool,
    },
    /// Give a site its own Redis ACL user limited to its key prefix.
    SiteRedis { domain: String },
    /// Run a queue worker for a site under supervisor.
    SiteQueue {
        domain: String,
        #[arg(long, default_value = "redis")]
        connection: String,
        #[arg(long, default_value_t = 1)]
        processes: u8,
    },
    /// Delete a site created by Sesame (backs up its database first).
    RemoveSite {
        domain: String,
        #[arg(long)]
        db: Option<String>,
    },
    /// Run Reverb under supervisor for the site with this domain.
    Reverb { domain: String, #[arg(long, default_value_t = 8080)] port: u16 },
}

#[derive(Subcommand)]
pub enum Command {
    /// List saved servers.
    Servers,
    /// Save a server: `sesame add prod root@1.2.3.4` (uses ssh-agent unless --key).
    Add {
        name: String,
        /// user@host[:port]
        target: String,
        /// Private key file instead of ssh-agent.
        #[arg(long)]
        key: Option<String>,
    },
    /// Remove a saved server.
    Remove { name: String },
    /// Discover a saved server (by name) or an ad-hoc user@host[:port]; prints JSON.
    Discover {
        server: String,
        #[arg(long)]
        key: Option<String>,
    },
    /// List the operations Sesame exposes to agents (MCP/REST).
    Ops,
    /// Change a saved server: prints the plan, runs it with a live log, records it in the audit log.
    Do {
        server: String,
        #[command(subcommand)]
        action: DoAction,
        /// Run without asking. Destructive actions also need --confirm <name>.
        #[arg(long, global = true)]
        yes: bool,
        /// The name a destructive action asks for (database, service).
        #[arg(long, global = true)]
        confirm: Option<String>,
    },
    /// Save a credential for a server in the encrypted vault; the value is read from stdin.
    SecretSet { server: String, name: String },
    /// Show the audit log.
    Audit {
        server: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Check for a newer release; `--install` downloads, verifies and installs it.
    Update {
        #[arg(long)]
        install: bool,
        /// stable or beta (default: beta for pre-release builds)
        #[arg(long)]
        channel: Option<String>,
    },
    /// Attach a snapshot JSON (output of `sesame discover`) to a saved server.
    ImportSnapshot { name: String, file: String },
}

/// Unlocks the encrypted vault for this process: `SESAME_MASTER_PASSWORD`,
/// the key file, or a prompt on the terminal. Creates the vault on first use.
pub fn unlock_vault(store: &std::sync::Arc<SqliteStore>) -> Result<()> {
    use sesame_core::vault::{Vault, VaultMode};
    if sesame_core::secrets::is_unlocked() {
        return Ok(());
    }
    let from_env = std::env::var("SESAME_MASTER_PASSWORD").ok();
    let vault = match Vault::mode(store)? {
        VaultMode::KeyFile => Vault::unlock_with_key_file(store.clone(), &Vault::key_file_path()?)?,
        VaultMode::Password => {
            let pw = match from_env {
                Some(p) => p,
                None => rpassword::prompt_password("Sesame master password: ")?,
            };
            Vault::unlock_with_password(store.clone(), &pw)?
        }
        VaultMode::New => {
            let pw = match from_env {
                Some(p) => p,
                None => {
                    eprintln!("Create a master password for Sesame's encrypted vault.");
                    let a = rpassword::prompt_password("New master password: ")?;
                    let b = rpassword::prompt_password("Repeat: ")?;
                    if a != b {
                        bail!("passwords do not match");
                    }
                    a
                }
            };
            Vault::create_with_password(store.clone(), &pw)?
        }
    };
    sesame_core::secrets::install(vault);
    Ok(())
}

pub async fn run(cmd: Command) -> Result<()> {
    let store = std::sync::Arc::new(SqliteStore::open_default()?);
    let needs_vault = !matches!(cmd, Command::Servers | Command::Ops | Command::Audit { .. } | Command::Update { .. } | Command::Remove { .. });
    if needs_vault {
        unlock_vault(&store)?;
    }
    match cmd {
        Command::Servers => {
            for s in store.servers()? {
                println!("{}\t{}", s.name, s.address());
            }
        }
        Command::Add { name, target, key } => {
            let server = parse_target(&name, &target, key)?;
            store.save_server(&server)?;
            println!("saved {} ({})", server.name, server.address());
        }
        Command::Remove { name } => {
            let s = find(&store, &name)?.with_context(|| format!("no server named {name}"))?;
            store.delete_server(s.id)?;
            println!("removed {name}");
        }
        Command::Discover { server, key } => {
            let saved = find(&store, &server)?;
            let config = match &saved {
                Some(s) => s.clone(),
                None => parse_target(&server, &server, key)?,
            };
            eprintln!("connecting to {} …", config.address());
            let (snap, host_key) = sesame_discovery::discover_server(&config).await?;
            if let sesame_ssh::HostKeyStatus::Learned { fingerprint } = host_key {
                eprintln!("new host key trusted and saved to known_hosts: {fingerprint}");
            }
            if saved.is_some() {
                store.save_snapshot(config.id, &snap)?;
            }
            println!("{}", serde_json::to_string_pretty(&snap)?);
        }
        Command::ImportSnapshot { name, file } => {
            let s = find(&store, &name)?.with_context(|| format!("no server named {name}"))?;
            let snap = serde_json::from_str(&std::fs::read_to_string(&file)?)?;
            store.save_snapshot(s.id, &snap)?;
            println!("imported snapshot for {name}");
        }
        Command::Update { install, channel } => {
            let current = sesame_update::current_version();
            let channel = channel
                .as_deref()
                .or(store.setting("update.channel")?.as_deref())
                .and_then(sesame_update::Channel::parse)
                .unwrap_or_else(|| sesame_update::Channel::default_for(&current));
            println!("current: {current} ({} channel)", channel.as_str());
            let Some(info) = sesame_update::check(channel).await? else {
                println!("you are up to date");
                return Ok(());
            };
            println!("available: {}\n", info.latest.version);
            for r in &info.changelog {
                println!("## {}\n{}\n", r.version, r.notes.trim());
            }
            if install {
                let dir = tempfile_dir()?;
                let archive = sesame_update::download(&info.latest, &dir).await?;
                let target = sesame_update::install(&archive)?;
                println!("installed into {} — restart Sesame", target.display());
            }
        }
        Command::Do { server, action, yes, confirm } => {
            let config = find(&store, &server)?.with_context(|| format!("no saved server named {server}"))?;
            let snap = store.snapshot(config.id)?;
            let redis_admin = sesame_core::secrets::get_named(config.id, "REDIS_PASSWORD").ok().flatten();
            let plan = build_plan(&action, snap.as_ref(), mongo_admin(config.id), redis_admin)?;
            run_plan(&store, &config, plan, yes, confirm.as_deref()).await?;
        }
        Command::SecretSet { server, name } => {
            let config = find(&store, &server)?.with_context(|| format!("no server named {server}"))?;
            let mut value = String::new();
            std::io::stdin().read_line(&mut value)?;
            sesame_core::secrets::set_named(config.id, &name, value.trim())?;
            println!("saved {name} for {server}");
        }
        Command::Audit { server, limit } => {
            let id = match server {
                Some(name) => Some(find(&store, &name)?.with_context(|| format!("no server named {name}"))?.id),
                None => None,
            };
            for e in store.audit(id, limit)? {
                println!("{}  {:<4} {:<6} {:<18} {}", e.at, if e.ok { "ok" } else { "FAIL" }, e.actor, e.op, e.title);
            }
        }
        Command::Ops => {
            for op in sesame_core::ops::OPS {
                println!("{:<16} {:<12} {}", op.name, format!("{:?}", op.risk), op.description);
            }
        }
    }
    Ok(())
}

fn tempfile_dir() -> Result<std::path::PathBuf> {
    let dir = std::env::temp_dir().join("sesame-update");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn find(store: &SqliteStore, name: &str) -> Result<Option<ServerConfig>> {
    Ok(store.servers()?.into_iter().find(|s| s.name == name))
}

pub fn parse_target(name: &str, target: &str, key: Option<String>) -> Result<ServerConfig> {
    let (user, rest) = target.split_once('@').unwrap_or(("root", target));
    let (host, port) = match rest.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().context("bad port")?),
        None => (rest, 22),
    };
    if host.is_empty() || user.is_empty() {
        bail!("expected user@host[:port], got {target}");
    }
    let mut s = ServerConfig::new(name, host, user);
    s.port = port;
    if let Some(path) = key {
        s.auth = AuthMethod::KeyFile { path };
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_targets() {
        let s = parse_target("p", "deploy@10.0.0.5:2222", None).unwrap();
        assert_eq!((s.user.as_str(), s.host.as_str(), s.port), ("deploy", "10.0.0.5", 2222));
        let s = parse_target("p", "example.com", Some("~/.ssh/id".into())).unwrap();
        assert_eq!((s.user.as_str(), s.port), ("root", 22));
        assert_eq!(s.auth, AuthMethod::KeyFile { path: "~/.ssh/id".into() });
        assert!(parse_target("p", "@host", None).is_err());
    }
}

fn engine(s: &str) -> Result<sesame_core::snapshot::DatabaseKind> {
    use sesame_core::snapshot::DatabaseKind::*;
    Ok(match s {
        "mysql" => MySql,
        "mariadb" => MariaDb,
        "postgres" | "postgresql" | "pgsql" => Postgres,
        "mongodb" | "mongo" => MongoDb,
        _ => bail!("unknown engine {s}"),
    })
}

fn service(s: &str) -> Result<sesame_provision::Service> {
    sesame_provision::Service::ALL
        .into_iter()
        .find(|svc| svc.slug() == s || (s == "s3" && *svc == sesame_provision::Service::RustFs) || (s == "postgres" && *svc == sesame_provision::Service::Postgres))
        .with_context(|| format!("unknown service {s}"))
}

/// The MongoDB admin Sesame created when it installed MongoDB, if any.
pub fn mongo_admin(server: sesame_core::ServerId) -> Option<sesame_ops::actions::Admin> {
    let get = |k: &str| match sesame_core::secrets::get_named(server, k) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("keychain: cannot read {k}: {e:#}");
            None
        }
    };
    Some(sesame_ops::actions::Admin { user: get("MONGO_ADMIN_USER")?, password: get("MONGO_ADMIN_PASSWORD")? })
}

fn site_app(snap: &sesame_core::ServerSnapshot, domain: &str) -> Result<String> {
    snap.sites
        .iter()
        .find(|s| s.domains.iter().any(|d| d == domain))
        .and_then(|s| s.app_path.clone())
        .with_context(|| format!("no site {domain} in the last discovery"))
}

fn build_plan(
    action: &DoAction,
    snap: Option<&sesame_core::ServerSnapshot>,
    admin: Option<sesame_ops::actions::Admin>,
    redis_admin: Option<String>,
) -> Result<sesame_ops::Plan> {
    use sesame_ops::actions::{self, ServiceAction, ServiceKind};
    match action {
        DoAction::Install { service: s } => {
            let snap = snap.context("discover the server first: sesame discover <name>")?;
            sesame_provision::install(service(s)?, snap)
        }
        DoAction::Uninstall { service: s, delete_data } => sesame_provision::uninstall(service(s)?, !delete_data),
        DoAction::DbCreate { engine: e, name, user } => actions::db_create(engine(e)?, name, user.as_deref().unwrap_or(name), admin.as_ref()),
        DoAction::DbDrop { engine: e, name, user } => actions::db_drop(engine(e)?, name, user.as_deref(), admin.as_ref()),
        DoAction::Service { kind, name, action } => {
            let kind = match kind.as_str() {
                "systemd" => ServiceKind::Systemd,
                "supervisor" => ServiceKind::Supervisor,
                k => bail!("kind must be systemd or supervisor, not {k}"),
            };
            let action = match action.as_str() {
                "start" => ServiceAction::Start,
                "stop" => ServiceAction::Stop,
                "restart" => ServiceAction::Restart,
                "enable" => ServiceAction::Enable,
                "disable" => ServiceAction::Disable,
                a => bail!("unknown action {a}"),
            };
            actions::service(kind, name, action)
        }
        DoAction::Maintenance { path, state } => actions::maintenance(path, state == "on"),
        DoAction::SiteRedis { domain } => {
            let snap = snap.context("discover the server first")?;
            let app = site_app(snap, domain)?;
            let admin = redis_admin.as_deref();
            sesame_ops::site::redis_for_site(domain, &app, admin)
        }
        DoAction::SiteQueue { domain, connection, processes } => {
            let snap = snap.context("discover the server first")?;
            let app = site_app(snap, domain)?;
            sesame_ops::site::queue_for_site(domain, &app, connection, *processes)
        }
        DoAction::RemoveSite { domain, db } => {
            let snap = snap.context("discover the server first")?;
            sesame_ops::site::remove_site(domain, db.as_deref().map(engine).transpose()?, snap)
        }
        DoAction::NewSite { domain, kind, runtime, repo, branch, root, db, alias, no_scheduler } => {
            use sesame_ops::site::{NewSite, Runtime, SiteKind, Source};
            let snap = snap.context("discover the server first")?;
            let kind = match kind.as_str() {
                "laravel" => SiteKind::Laravel,
                "static" => SiteKind::Static,
                k => bail!("kind must be laravel or static, not {k}"),
            };
            let site = NewSite {
                domain: domain.clone(),
                aliases: alias.clone(),
                kind,
                runtime: if runtime == "octane" { Runtime::Octane } else { Runtime::Fpm },
                source: repo.clone().map(|repo| Source { repo, branch: branch.clone() }),
                web_root: root.clone().unwrap_or_else(|| if kind == SiteKind::Laravel { "public".into() } else { ".".into() }),
                database: db.as_deref().map(engine).transpose()?,
                scheduler: !no_scheduler,
            };
            sesame_ops::site::new_site(&site, snap)
        }
        DoAction::Cert { name, cert, key } => {
            let cert = std::fs::read_to_string(cert).with_context(|| format!("read {cert}"))?;
            let key = std::fs::read_to_string(key).with_context(|| format!("read {key}"))?;
            actions::cert_install(name, &cert, &key)
        }
        DoAction::Reverb { domain, port } => {
            let snap = snap.context("discover the server first")?;
            let site = snap
                .sites
                .iter()
                .find(|s| s.domains.iter().any(|d| d == domain))
                .with_context(|| format!("no site {domain} in the last discovery"))?;
            sesame_provision::reverb_for_site(site, *port)
        }
    }
}

async fn run_plan(
    store: &SqliteStore,
    server: &ServerConfig,
    plan: sesame_ops::Plan,
    yes: bool,
    confirm: Option<&str>,
) -> Result<()> {
    use sesame_core::ops::Risk;
    eprintln!("{} on {} ({:?})", plan.title, server.address(), plan.risk);
    for d in &plan.details {
        eprintln!("  • {d}");
    }
    if !yes {
        bail!("re-run with --yes to apply");
    }
    if plan.risk == Risk::Destructive && confirm != plan.confirm_word.as_deref() {
        bail!("destructive: add --confirm {}", plan.confirm_word.clone().unwrap_or_default());
    }
    let session = sesame_ssh::Session::connect(server).await?;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let printer = tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            match ev {
                sesame_ops::Event::Step { index, title } => eprintln!("\n▶ {}. {title}", index + 1),
                sesame_ops::Event::Line(l) => eprintln!("  {l}"),
                sesame_ops::Event::Done { ok, error } => {
                    eprintln!("\n{}", if ok { "✓ done".to_string() } else { format!("✗ {}", error.unwrap_or_default()) })
                }
            }
        }
    });
    let result = sesame_ops::execute(&session, &plan, Some(tx)).await;
    let _ = printer.await;
    let _ = session.close().await;
    let (ok, log) = match &result {
        Ok(out) => (true, out.tail.clone()),
        Err(e) => (false, format!("{e:#}")),
    };
    store.log_audit(&sesame_core::AuditEntry {
        at: sesame_discovery::parse::app::now_rfc3339(),
        server_id: server.id,
        op: plan.op.to_string(),
        title: plan.title.clone(),
        actor: "cli".into(),
        ok,
        log,
    })?;
    let out = result?;
    for (k, v) in &out.reveal {
        let _ = sesame_core::secrets::set_named(server.id, &plan.secret_name(k), v);
        println!("{k}={v}");
    }
    Ok(())
}
