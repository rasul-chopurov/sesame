//! A server's shell in the browser: a PTY over SSH bridged to a WebSocket.
//! Admins only; every session is written to the audit log.
//!
//! Browsers cannot set headers on a WebSocket, so the session token travels
//! as the second subprotocol: `new WebSocket(url, ["sesame", token])`.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use sesame_core::{AuditEntry, Store as _};
use sesame_ssh::{Shell, ShellInput};

use crate::api::{App, user_for};
use crate::users::Role;

#[derive(Deserialize)]
pub struct Params {
    server: String,
    cols: Option<u16>,
    rows: Option<u16>,
}

pub async fn upgrade(State(app): State<App>, headers: HeaderMap, Query(p): Query<Params>, ws: WebSocketUpgrade) -> Response {
    let token = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').map(str::trim).nth(1))
        .unwrap_or_default()
        .to_string();
    let Some(user) = user_for(&app, &token) else { return (StatusCode::UNAUTHORIZED, "sign in first").into_response() };
    if user.role < Role::Admin || user.agent {
        return (StatusCode::FORBIDDEN, "the terminal needs the admin role").into_response();
    }
    let Some(server) = app.store.servers().unwrap_or_default().into_iter().find(|s| s.name == p.server) else {
        return (StatusCode::NOT_FOUND, "no such server").into_response();
    };
    let (cols, rows) = (p.cols.unwrap_or(120).clamp(20, 400), p.rows.unwrap_or(32).clamp(5, 200));
    ws.protocols(["sesame"]).on_upgrade(move |socket| async move {
        let _ = app.store.log_audit(&AuditEntry {
            at: time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_default(),
            server_id: server.id,
            op: "terminal".into(),
            title: format!("terminal opened by {}", user.email),
            actor: format!("user:{}", user.email),
            ok: true,
            log: String::new(),
        });
        bridge(socket, server, cols, rows).await;
    })
}

/// Text frames from the browser are keystrokes, except `\u{1}resize:<cols>x<rows>`.
async fn bridge(socket: WebSocket, server: sesame_core::ServerConfig, cols: u16, rows: u16) {
    let (mut tx, mut rx) = socket.split();
    let mut shell = match Shell::open(&server, cols, rows).await {
        Ok(s) => s,
        Err(e) => {
            let _ = tx.send(Message::Text(format!("\r\nconnection failed: {e:#}\r\n").into())).await;
            return;
        }
    };
    loop {
        tokio::select! {
            out = shell.output.recv() => match out {
                Some(bytes) => if tx.send(Message::Binary(bytes.into())).await.is_err() { break },
                None => { let _ = tx.send(Message::Text("\r\n[session ended]\r\n".into())).await; break }
            },
            msg = rx.next() => match msg {
                Some(Ok(Message::Text(t))) => {
                    if let Some(size) = t.strip_prefix("\u{1}resize:") {
                        if let Some((c, r)) = size.split_once('x') {
                            if let (Ok(c), Ok(r)) = (c.parse(), r.parse()) {
                                let _ = shell.input.send(ShellInput::Resize { cols: c, rows: r });
                            }
                        }
                    } else if shell.input.send(ShellInput::Data(t.as_bytes().to_vec())).is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Binary(b))) => if shell.input.send(ShellInput::Data(b.to_vec())).is_err() { break },
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => { let _ = shell.input.send(ShellInput::Close); break }
                _ => {}
            },
        }
    }
}
