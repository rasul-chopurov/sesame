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

pub async fn run(cmd: Command) -> Result<()> {
    let store = SqliteStore::open_default()?;
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
