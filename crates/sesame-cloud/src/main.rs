//! `sesame-cloud`: Sesame for a team, on a server of your own.
//!
//! One binary serves the web app and its API. It manages servers with the
//! same engine as the desktop app, so plans, checks and the audit log are
//! identical. Put it behind Caddy for TLS; it listens on 127.0.0.1 by default.

mod api;
mod users;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use sesame_core::SqliteStore;
use sesame_core::vault::{Vault, VaultMode};

#[derive(Parser)]
#[command(name = "sesame-cloud", version, about = "Sesame for teams: web app and API")]
struct Args {
    /// Address to listen on.
    #[arg(long, env = "SESAME_CLOUD_LISTEN", default_value = "127.0.0.1:7880")]
    listen: String,
    /// Built web app (web/dist) to serve.
    #[arg(long, env = "SESAME_CLOUD_WEB")]
    web: Option<PathBuf>,
}

fn unlock(store: &Arc<SqliteStore>) -> Result<()> {
    let vault = match Vault::mode(store)? {
        VaultMode::KeyFile => Vault::unlock_with_key_file(store.clone(), &Vault::key_file_path()?)?,
        VaultMode::Password => {
            let pw = std::env::var("SESAME_MASTER_PASSWORD").context("set SESAME_MASTER_PASSWORD to unlock the vault")?;
            Vault::unlock_with_password(store.clone(), &pw)?
        }
        VaultMode::New => {
            let pw = std::env::var("SESAME_MASTER_PASSWORD").context("set SESAME_MASTER_PASSWORD to create the vault")?;
            Vault::create_with_password(store.clone(), &pw)?
        }
    };
    sesame_core::secrets::install(vault);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let store = Arc::new(SqliteStore::open_default()?);
    unlock(&store)?;
    let dir = SqliteStore::data_dir()?;
    let users = Arc::new(users::Users::open(&dir.join("cloud.db"))?);
    let setup_token = users::random_token("")?[..12].to_string();
    if users.is_empty()? {
        eprintln!("First start: open the web app and create the owner account with this setup code: {setup_token}");
    }
    let app = api::App {
        engine: Arc::new(sesame_engine::Engine::new(store.clone())),
        store,
        users,
        setup_token: Arc::new(setup_token),
    };
    let listener = tokio::net::TcpListener::bind(&args.listen).await.with_context(|| format!("cannot listen on {}", args.listen))?;
    eprintln!("Sesame Cloud on http://{}", args.listen);
    axum::serve(listener, api::router(app, args.web)).await?;
    Ok(())
}
