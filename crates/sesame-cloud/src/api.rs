//! The HTTP API behind the web app, also used by agents with a token.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use sesame_core::ops::Risk;
use sesame_core::{SqliteStore, Store as _};
use sesame_engine::{Actor, Consent, Engine, TOOLS};

use crate::users::{Role, User, Users};

#[derive(Clone)]
pub struct App {
    pub engine: Arc<Engine>,
    pub store: Arc<SqliteStore>,
    pub users: Arc<Users>,
    /// Needed once to create the first owner.
    pub setup_token: Arc<String>,
}

type Reply = (StatusCode, Json<Value>);

fn err(status: StatusCode, msg: impl Into<String>) -> Reply {
    (status, Json(json!({ "error": msg.into() })))
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers.get("authorization")?.to_str().ok()?.strip_prefix("Bearer ")
}

pub(crate) fn user_for(app: &App, token: &str) -> Option<User> {
    app.users.by_token(token).ok().flatten()
}

fn whoami(app: &App, headers: &HeaderMap) -> Result<User, Reply> {
    let token = bearer(headers).ok_or_else(|| err(StatusCode::UNAUTHORIZED, "sign in first"))?;
    app.users
        .by_token(token)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::UNAUTHORIZED, "your session has ended — sign in again"))
}

fn need(user: &User, role: Role) -> Result<(), Reply> {
    if user.role >= role { Ok(()) } else { Err(err(StatusCode::FORBIDDEN, format!("this needs the {} role", role.as_str()))) }
}

pub fn router(app: App, web_dir: Option<std::path::PathBuf>) -> Router {
    let api = Router::new()
        .route("/api/health", get(|| async { Json(json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") })) }))
        .route("/api/setup", get(setup_needed).post(setup))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/me", get(me))
        .route("/api/tools", get(tools))
        .route("/api/tools/{name}", post(call))
        .route("/api/approvals", get(approvals))
        .route("/api/approvals/{id}/{decision}", post(decide))
        .route("/api/audit", get(audit))
        .route("/api/users", get(list_users).post(create_user))
        .route("/api/users/{id}", delete(delete_user).patch(set_role))
        .route("/api/agents", post(create_agent))
        .route("/api/terminal", get(crate::terminal::upgrade))
        .with_state(app);
    match web_dir {
        Some(dir) => {
            let index = dir.join("index.html");
            api.fallback_service(tower_http::services::ServeDir::new(dir).fallback(tower_http::services::ServeFile::new(index)))
        }
        None => api,
    }
}

async fn setup_needed(State(app): State<App>) -> Reply {
    (StatusCode::OK, Json(json!({ "needed": app.users.is_empty().unwrap_or(false) })))
}

#[derive(Deserialize)]
struct Setup {
    setup_token: String,
    email: String,
    name: String,
    password: String,
}

async fn setup(State(app): State<App>, Json(b): Json<Setup>) -> Reply {
    if !app.users.is_empty().unwrap_or(false) {
        return err(StatusCode::CONFLICT, "Sesame Cloud is already set up");
    }
    // Forgive pasted spaces, quotes or upper case: only the code's characters count.
    let given: String = b.setup_token.chars().filter(|c| c.is_ascii_hexdigit()).collect::<String>().to_lowercase();
    if given != app.setup_token.as_str() {
        tokio::time::sleep(Duration::from_secs(1)).await;
        return err(StatusCode::FORBIDDEN, "wrong setup code — it is printed in the server log at start");
    }
    match app.users.create(&b.email, &b.name, Role::Owner, &b.password).and_then(|_| app.users.login(&b.email, &b.password)) {
        Ok(Some((user, token))) => (StatusCode::OK, Json(json!({ "token": token, "user": user }))),
        Ok(None) => err(StatusCode::INTERNAL_SERVER_ERROR, "could not sign in"),
        Err(e) => err(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

#[derive(Deserialize)]
struct Login {
    email: String,
    password: String,
}

async fn login(State(app): State<App>, Json(b): Json<Login>) -> Reply {
    match app.users.login(&b.email, &b.password) {
        Ok(Some((user, token))) => (StatusCode::OK, Json(json!({ "token": token, "user": user }))),
        Ok(None) => {
            // Slow down guessing.
            tokio::time::sleep(Duration::from_millis(800)).await;
            err(StatusCode::UNAUTHORIZED, "wrong email or password")
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn logout(State(app): State<App>, headers: HeaderMap) -> Reply {
    if let Some(t) = bearer(&headers) {
        let _ = app.users.logout(t);
    }
    (StatusCode::OK, Json(json!({ "ok": true })))
}

async fn me(State(app): State<App>, headers: HeaderMap) -> Reply {
    match whoami(&app, &headers) {
        Ok(u) => (StatusCode::OK, Json(json!(u))),
        Err(e) => e,
    }
}

async fn tools(State(app): State<App>, headers: HeaderMap) -> Reply {
    if let Err(e) = whoami(&app, &headers) {
        return e;
    }
    let list: Vec<Value> = TOOLS
        .iter()
        .map(|t| json!({ "name": t.name, "description": t.description, "risk": t.risk, "input_schema": t.input_schema() }))
        .collect();
    (StatusCode::OK, Json(json!(list)))
}

/// Who may run what, and whether someone else has to approve it first.
pub fn consent_for(user: &User, tool: &str, risk: Risk) -> Result<Consent, String> {
    let ask = Consent::AskInApp { timeout: Duration::from_secs(900) };
    let admin_only = ["run_command", "add_server", "remove_server", "set_integration", "remove_integration"];
    if admin_only.contains(&tool) && (user.role < Role::Admin || user.agent) {
        return Err(format!("{tool} needs the admin role"));
    }
    // Site files include .env and code: not for viewers.
    if tool.starts_with("files_") && user.role < Role::Developer {
        return Err("site files need the developer role".into());
    }
    match risk {
        Risk::Read => Ok(Consent::Given),
        _ if user.role < Role::Developer => Err("viewers can only read".into()),
        // Agents never change anything without a person saying yes.
        _ if user.agent => Ok(ask),
        Risk::Destructive if user.role < Role::Admin => Ok(ask),
        _ => Ok(Consent::Given),
    }
}

async fn call(State(app): State<App>, headers: HeaderMap, Path(name): Path<String>, body: Option<Json<Value>>) -> Reply {
    let user = match whoami(&app, &headers) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let Some(tool) = TOOLS.iter().find(|t| t.name == name) else { return err(StatusCode::NOT_FOUND, format!("no tool {name}")) };
    let consent = match consent_for(&user, &name, tool.risk) {
        Ok(c) => c,
        Err(m) => return err(StatusCode::FORBIDDEN, m),
    };
    let actor = Actor(if user.agent { format!("agent:{}", user.name) } else { format!("user:{}", user.email) });
    let args = body.map(|Json(v)| v).unwrap_or(json!({}));
    match app.engine.call(&actor, &name, args, consent).await {
        Ok(v) => (StatusCode::OK, Json(v)),
        Err(e) => err(StatusCode::BAD_REQUEST, format!("{e:#}")),
    }
}

async fn approvals(State(app): State<App>, headers: HeaderMap) -> Reply {
    if let Err(e) = whoami(&app, &headers) {
        return e;
    }
    let servers = app.store.servers().unwrap_or_default();
    let list: Vec<Value> = app
        .store
        .pending_approvals()
        .unwrap_or_default()
        .into_iter()
        .map(|a| {
            let server = servers.iter().find(|s| s.id == a.server_id).map(|s| s.name.clone());
            json!({ "id": a.id, "created_at": a.created_at, "actor": a.actor, "server": server, "title": a.title, "details": a.details, "risk": a.risk })
        })
        .collect();
    (StatusCode::OK, Json(json!(list)))
}

async fn decide(State(app): State<App>, headers: HeaderMap, Path((id, decision)): Path<(String, String)>) -> Reply {
    let user = match whoami(&app, &headers) {
        Ok(u) => u,
        Err(e) => return e,
    };
    if user.agent {
        return err(StatusCode::FORBIDDEN, "agents cannot approve");
    }
    if let Err(e) = need(&user, Role::Admin) {
        return e;
    }
    let status = match decision.as_str() {
        "allow" => "approved",
        "deny" => "denied",
        _ => return err(StatusCode::BAD_REQUEST, "allow or deny"),
    };
    match app.store.decide_approval(&id, status) {
        Ok(true) => (StatusCode::OK, Json(json!({ "ok": true, "status": status }))),
        Ok(false) => err(StatusCode::CONFLICT, "already decided or expired"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize)]
struct AuditQuery {
    limit: Option<usize>,
}

async fn audit(State(app): State<App>, headers: HeaderMap, Query(q): Query<AuditQuery>) -> Reply {
    if let Err(e) = whoami(&app, &headers) {
        return e;
    }
    let servers = app.store.servers().unwrap_or_default();
    let list: Vec<Value> = app
        .store
        .audit(None, q.limit.unwrap_or(50).min(500))
        .unwrap_or_default()
        .into_iter()
        .map(|a| {
            let server = servers.iter().find(|s| s.id == a.server_id).map(|s| s.name.clone());
            json!({ "at": a.at, "server": server, "op": a.op, "title": a.title, "actor": a.actor, "ok": a.ok })
        })
        .collect();
    (StatusCode::OK, Json(json!(list)))
}

async fn list_users(State(app): State<App>, headers: HeaderMap) -> Reply {
    let user = match whoami(&app, &headers) {
        Ok(u) => u,
        Err(e) => return e,
    };
    if let Err(e) = need(&user, Role::Admin) {
        return e;
    }
    (StatusCode::OK, Json(json!(app.users.list().unwrap_or_default())))
}

#[derive(Deserialize)]
struct NewUser {
    email: String,
    name: String,
    role: String,
    password: String,
}

/// Admins add developers and viewers; only the owner adds admins.
fn may_grant(actor: &User, role: Role) -> bool {
    !actor.agent && (actor.role == Role::Owner || (actor.role == Role::Admin && role < Role::Admin)) && role != Role::Owner
}

async fn create_user(State(app): State<App>, headers: HeaderMap, Json(b): Json<NewUser>) -> Reply {
    let user = match whoami(&app, &headers) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let Some(role) = Role::parse(&b.role) else { return err(StatusCode::BAD_REQUEST, "role: viewer, developer or admin") };
    if !may_grant(&user, role) {
        return err(StatusCode::FORBIDDEN, "you cannot give this role");
    }
    match app.users.create(&b.email, &b.name, role, &b.password) {
        Ok(u) => (StatusCode::OK, Json(json!(u))),
        Err(e) => err(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

#[derive(Deserialize)]
struct NewAgent {
    name: String,
    role: String,
}

async fn create_agent(State(app): State<App>, headers: HeaderMap, Json(b): Json<NewAgent>) -> Reply {
    let user = match whoami(&app, &headers) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let Some(role) = Role::parse(&b.role).filter(|r| *r <= Role::Developer) else {
        return err(StatusCode::BAD_REQUEST, "agents can be viewers or developers");
    };
    if !may_grant(&user, role) {
        return err(StatusCode::FORBIDDEN, "you cannot add agents");
    }
    match app.users.create_agent(&b.name, role) {
        Ok((u, token)) => (StatusCode::OK, Json(json!({ "user": u, "token": token }))),
        Err(e) => err(StatusCode::BAD_REQUEST, e.to_string()),
    }
}

#[derive(Deserialize)]
struct RoleChange {
    role: String,
}

async fn set_role(State(app): State<App>, headers: HeaderMap, Path(id): Path<i64>, Json(b): Json<RoleChange>) -> Reply {
    let user = match whoami(&app, &headers) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let Some(role) = Role::parse(&b.role) else { return err(StatusCode::BAD_REQUEST, "unknown role") };
    let Ok(Some(target)) = app.users.get(id) else { return err(StatusCode::NOT_FOUND, "no such user") };
    if target.id == user.id || target.role == Role::Owner || !may_grant(&user, role) || (user.role != Role::Owner && target.role >= Role::Admin) {
        return err(StatusCode::FORBIDDEN, "you cannot change this role");
    }
    match app.users.set_role(id, role) {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn delete_user(State(app): State<App>, headers: HeaderMap, Path(id): Path<i64>) -> Reply {
    let user = match whoami(&app, &headers) {
        Ok(u) => u,
        Err(e) => return e,
    };
    let Ok(Some(target)) = app.users.get(id) else { return err(StatusCode::NOT_FOUND, "no such user") };
    let allowed = !user.agent
        && target.id != user.id
        && target.role != Role::Owner
        && (user.role == Role::Owner || (user.role == Role::Admin && target.role < Role::Admin));
    if !allowed {
        return err(StatusCode::FORBIDDEN, "you cannot remove this account");
    }
    match app.users.delete(id) {
        Ok(()) => (StatusCode::OK, Json(json!({ "ok": true }))),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(role: Role, agent: bool) -> User {
        User { id: 1, email: "a@b.c".into(), name: "a".into(), role, agent }
    }

    #[test]
    fn roles_decide_who_runs_what() {
        let given = |c: Result<Consent, String>| matches!(c, Ok(Consent::Given));
        let asks = |c: Result<Consent, String>| matches!(c, Ok(Consent::AskInApp { .. }));
        assert!(given(consent_for(&u(Role::Viewer, false), "list_sites", Risk::Read)));
        assert!(consent_for(&u(Role::Viewer, false), "install_service", Risk::Change).is_err());
        assert!(given(consent_for(&u(Role::Developer, false), "install_service", Risk::Change)));
        assert!(asks(consent_for(&u(Role::Developer, false), "drop_database", Risk::Destructive)));
        assert!(given(consent_for(&u(Role::Admin, false), "drop_database", Risk::Destructive)));
        assert!(asks(consent_for(&u(Role::Developer, true), "install_service", Risk::Change)), "agents always ask");
        assert!(consent_for(&u(Role::Developer, false), "run_command", Risk::Destructive).is_err());
        assert!(consent_for(&u(Role::Viewer, false), "files_read", Risk::Read).is_err());
        assert!(consent_for(&u(Role::Admin, true), "add_server", Risk::Change).is_err(), "agents never add servers");
        assert!(may_grant(&u(Role::Admin, false), Role::Developer));
        assert!(!may_grant(&u(Role::Admin, false), Role::Admin));
        assert!(may_grant(&u(Role::Owner, false), Role::Admin));
        assert!(!may_grant(&u(Role::Owner, false), Role::Owner));
    }
}
