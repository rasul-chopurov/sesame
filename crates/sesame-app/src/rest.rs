//! `sesame serve`: a local REST API for scripts and agents.
//!
//! - `GET  /v1/tools`        tool list with JSON schemas
//! - `POST /v1/tools/{name}` call a tool with a JSON body of arguments
//!
//! Listens on 127.0.0.1 only and needs `Authorization: Bearer <token>`. Only a
//! SHA-256 of the token is stored; changes wait for approval in the app.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{Value, json};
use sesame_core::{SqliteStore, Store as _};
use sesame_engine::{Actor, Consent, Engine, TOOLS};
use sha2::{Digest, Sha256};

#[derive(Clone)]
struct App {
    engine: Arc<Engine>,
    token_hash: String,
}

fn hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Makes a new API token, stores its hash and returns the token (shown once).
pub fn new_token(store: &SqliteStore) -> Result<String> {
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("{e}"))?;
    let token = format!("ses_{}", hex::encode(bytes));
    store.set_setting("api.token_sha256", &hash(&token))?;
    Ok(token)
}

fn authorized(app: &App, headers: &HeaderMap) -> bool {
    let Some(given) = headers.get("authorization").and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")) else {
        return false;
    };
    // Compare digests so timing does not reveal the token.
    let a = hash(given);
    a.len() == app.token_hash.len() && a.bytes().zip(app.token_hash.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn tools(State(app): State<App>, headers: HeaderMap) -> (StatusCode, Json<Value>) {
    if !authorized(&app, &headers) {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "missing or wrong bearer token" })));
    }
    let list: Vec<Value> = TOOLS
        .iter()
        .map(|t| json!({ "name": t.name, "description": t.description, "risk": t.risk, "input_schema": t.input_schema() }))
        .collect();
    (StatusCode::OK, Json(json!(list)))
}

async fn call(State(app): State<App>, headers: HeaderMap, Path(name): Path<String>, body: Option<Json<Value>>) -> (StatusCode, Json<Value>) {
    if !authorized(&app, &headers) {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "missing or wrong bearer token" })));
    }
    let args = body.map(|Json(v)| v).unwrap_or(json!({}));
    let actor = Actor("agent:rest".into());
    match app.engine.call(&actor, &name, args, Consent::AskInApp { timeout: Duration::from_secs(600) }).await {
        Ok(v) => (StatusCode::OK, Json(v)),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("{e:#}") }))),
    }
}

pub async fn serve(store: Arc<SqliteStore>, port: u16) -> Result<()> {
    let token_hash = store.setting("api.token_sha256")?.ok_or_else(|| anyhow::anyhow!("no API token yet: run `sesame api-token`"))?;
    let _ = crate::cli::unlock_vault_quietly(&store);
    let app = App { engine: Arc::new(Engine::new(store)), token_hash };
    let router = Router::new()
        .route("/v1/health", get(|| async { Json(json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") })) }))
        .route("/v1/tools", get(tools))
        .route("/v1/tools/{name}", post(call))
        .with_state(app);
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    eprintln!("Sesame REST API on http://127.0.0.1:{port}/v1 (bearer token required)");
    axum::serve(listener, router).await?;
    Ok(())
}
