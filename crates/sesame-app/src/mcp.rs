//! `sesame mcp`: a Model Context Protocol server over stdio for Claude Code,
//! Claude Desktop, Cursor and other agents. Tools come from sesame-engine;
//! reading is immediate, changes wait for approval in the Sesame app.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde_json::{Value, json};
use sesame_core::SqliteStore;
use sesame_engine::{Actor, Consent, Engine, TOOLS};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const PROTOCOL: &str = "2025-06-18";

pub async fn serve(store: Arc<SqliteStore>) -> Result<()> {
    // stdin/stdout carry the protocol, so never prompt: use the key file or
    // SESAME_MASTER_PASSWORD; without either, tools that need secrets say so.
    if let Err(e) = crate::cli::unlock_vault_quietly(&store) {
        eprintln!("sesame mcp: vault locked ({e:#}); set SESAME_MASTER_PASSWORD in the MCP config to use saved secrets");
    }
    let engine = Arc::new(Engine::new(store));
    let mut actor = Actor("agent:mcp".into());
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                write(&mut stdout, json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": e.to_string() } })).await?;
                continue;
            }
        };
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        // Notifications (no id) need no answer.
        let Some(id) = id else { continue };
        let reply = match method {
            "initialize" => {
                if let Some(name) = params.pointer("/clientInfo/name").and_then(Value::as_str) {
                    let clean: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || " -_.".contains(*c)).take(40).collect();
                    actor = Actor(format!("agent:{clean}"));
                }
                let version = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL);
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "sesame", "title": "Sesame", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": "Sesame manages the user's servers over SSH. Start with list_servers and get_server. Reading tools run at once. Tools that change something wait until the user approves them in the Sesame app — tell the user to look at Sesame. Prefer specific tools over run_command.",
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": TOOLS.iter().map(|t| json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema(),
                "annotations": {
                    "readOnlyHint": t.risk == sesame_core::ops::Risk::Read,
                    "destructiveHint": t.risk == sesame_core::ops::Risk::Destructive,
                },
            })).collect::<Vec<_>>() })),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                let consent = Consent::AskInApp { timeout: Duration::from_secs(600) };
                Ok(match engine.call(&actor, &name, args, consent).await {
                    Ok(v) => json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v)? }], "isError": false }),
                    Err(e) => json!({ "content": [{ "type": "text", "text": format!("{e:#}") }], "isError": true }),
                })
            }
            m => Err(json!({ "code": -32601, "message": format!("method not found: {m}") })),
        };
        let out = match reply {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
        };
        write(&mut stdout, out).await?;
    }
    Ok(())
}

async fn write(out: &mut tokio::io::Stdout, v: Value) -> Result<()> {
    let mut line = serde_json::to_vec(&v)?;
    line.push(b'\n');
    out.write_all(&line).await?;
    out.flush().await?;
    Ok(())
}
