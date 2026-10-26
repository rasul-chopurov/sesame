//! Sesame's operations as tools with JSON schemas. The CLI, the MCP server
//! and the REST API all call [`Engine::call`], so agents get exactly the
//! plans, checks, confirmations and audit log the app uses.
//!
//! Reading is immediate. Every change from an agent becomes a pending
//! approval that the user accepts or rejects in the app; nothing runs before.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};
use sesame_core::ops::Risk;
use sesame_core::snapshot::DatabaseKind;
use sesame_core::{Approval, AuditEntry, ServerConfig, SqliteStore, Store, secrets};
use sesame_ops::Plan;

pub mod tools;

pub use tools::{TOOLS, Tool};

/// Who is asking: `user`, `cli`, or `agent:<client name>`.
#[derive(Debug, Clone)]
pub struct Actor(pub String);

impl Actor {
    pub fn is_agent(&self) -> bool {
        self.0.starts_with("agent:")
    }
}

/// How changes get the user's consent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consent {
    /// Already confirmed (the CLI with --yes, the app's own dialog).
    Given,
    /// Ask in the app and wait (agents).
    AskInApp { timeout: Duration },
}

pub struct Engine {
    pub store: Arc<SqliteStore>,
}

fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    args.get(key).and_then(Value::as_str).with_context(|| format!("missing string argument `{key}`"))
}

fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

fn bool_arg(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(default)
}

pub fn engine_kind(s: &str) -> Result<DatabaseKind> {
    Ok(match s {
        "mysql" => DatabaseKind::MySql,
        "mariadb" => DatabaseKind::MariaDb,
        "postgres" | "postgresql" | "pgsql" => DatabaseKind::Postgres,
        "mongodb" | "mongo" => DatabaseKind::MongoDb,
        _ => bail!("unknown database engine `{s}` (mysql, mariadb, postgres, mongodb)"),
    })
}

impl Engine {
    pub fn new(store: Arc<SqliteStore>) -> Self {
        Self { store }
    }

    fn server(&self, name: &str) -> Result<ServerConfig> {
        self.store
            .servers()?
            .into_iter()
            .find(|s| s.name == name || s.host == name)
            .with_context(|| format!("no server named `{name}` — call list_servers"))
    }

    fn snapshot(&self, server: &ServerConfig) -> Result<sesame_core::ServerSnapshot> {
        self.store
            .snapshot(server.id)?
            .with_context(|| format!("`{}` was never discovered — call discover_server first", server.name))
    }

    /// Runs one tool with JSON arguments and returns its JSON result.
    pub async fn call(&self, actor: &Actor, name: &str, args: Value, consent: Consent) -> Result<Value> {
        let tool = tools::find(name).with_context(|| format!("unknown tool `{name}`"))?;
        match tool.risk {
            Risk::Read => self.read(name, &args).await,
            _ => {
                let (server, plan) = self.plan(name, &args).await?;
                self.execute(actor, server, plan, consent).await
            }
        }
    }

    async fn read(&self, name: &str, args: &Value) -> Result<Value> {
        match name {
            "list_servers" => {
                let mut out = Vec::new();
                for s in self.store.servers()? {
                    let snap = self.store.snapshot(s.id)?;
                    out.push(json!({
                        "name": s.name,
                        "address": s.address(),
                        "discovered_at": snap.as_ref().map(|x| x.taken_at.clone()),
                        "os": snap.as_ref().map(|x| x.os.distro.clone()),
                        "sites": snap.as_ref().map(|x| x.sites.len()),
                    }));
                }
                Ok(json!(out))
            }
            "discover_server" => {
                let server = self.server(str_arg(args, "server")?)?;
                let (snap, _) = sesame_discovery::discover_server(&server).await?;
                self.store.save_snapshot(server.id, &snap)?;
                Ok(summary(&snap))
            }
            "get_server" => {
                let server = self.server(str_arg(args, "server")?)?;
                Ok(summary(&self.snapshot(&server)?))
            }
            "get_snapshot" => {
                let server = self.server(str_arg(args, "server")?)?;
                Ok(serde_json::to_value(self.snapshot(&server)?)?)
            }
            "list_sites" => {
                let server = self.server(str_arg(args, "server")?)?;
                let snap = self.snapshot(&server)?;
                Ok(json!(snap.sites.iter().map(|s| json!({
                    "domains": s.domains,
                    "app_path": s.app_path,
                    "framework": s.framework,
                    "framework_version": s.framework_version,
                    "php": s.php,
                    "upstreams": s.upstreams,
                    "tls": s.tls,
                    "git": s.git,
                    "owner": s.owner,
                    "disabled": s.disabled,
                    "access": s.access,
                    "env": s.env,
                    "processes": s.processes,
                })).collect::<Vec<_>>()))
            }
            "list_databases" => {
                let server = self.server(str_arg(args, "server")?)?;
                Ok(serde_json::to_value(&self.snapshot(&server)?.databases)?)
            }
            "get_insights" => {
                let server = self.server(str_arg(args, "server")?)?;
                let list = sesame_discovery::insights::insights(&self.snapshot(&server)?, time::OffsetDateTime::now_utc());
                Ok(serde_json::to_value(list)?)
            }
            "list_services" => {
                let server = self.server(str_arg(args, "server")?)?;
                let snap = self.snapshot(&server)?;
                let mut out: Vec<Value> = sesame_provision::Service::ALL
                    .iter()
                    .map(|svc| {
                        let st = sesame_provision::detect(*svc, &snap);
                        json!({ "service": svc.slug(), "name": svc.name(), "installed": st.installed, "version": st.version, "conflict": st.conflict })
                    })
                    .collect();
                for p in sesame_provision::Panel::ALL {
                    out.push(json!({ "service": p.dir(), "name": p.name(), "installed": sesame_provision::panel_installed(p, &snap), "local_port": p.port() }));
                }
                Ok(json!(out))
            }
            "audit_log" => {
                let server = match opt_str(args, "server") {
                    Some(n) => Some(self.server(n)?.id),
                    None => None,
                };
                let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(20) as usize;
                Ok(serde_json::to_value(self.store.audit(server, limit)?)?)
            }
            "mail_domains" => {
                let rs = self.resend()?;
                let mut out = Vec::new();
                for d in rs.domains().await? {
                    out.push(rs.domain(&d.id).await.unwrap_or(d));
                }
                Ok(serde_json::to_value(out)?)
            }
            "integrations" => {
                let has = |k: &str| secrets::get_global(k).ok().flatten().is_some();
                let mut out = json!({ "github": null, "cloudflare": null, "resend": null });
                if has("GITHUB_TOKEN") {
                    out["github"] = match self.github()?.user().await {
                        Ok(u) => json!({ "connected": true, "login": u.login }),
                        Err(e) => json!({ "connected": false, "error": format!("{e:#}") }),
                    };
                }
                if has("CLOUDFLARE_TOKEN") {
                    let cf = self.cloudflare()?;
                    out["cloudflare"] = match cf.accounts().await {
                        Ok(a) => json!({ "connected": true, "account": a.first().map(|a| a.name.clone()) }),
                        Err(e) => json!({ "connected": false, "error": format!("{e:#}") }),
                    };
                }
                if has("RESEND_TOKEN") {
                    out["resend"] = match self.resend()?.domains().await {
                        Ok(d) => json!({ "connected": true, "domains": d.len() }),
                        Err(e) => json!({ "connected": false, "error": format!("{e:#}") }),
                    };
                }
                Ok(out)
            }
            "github_repos" => Ok(serde_json::to_value(self.github()?.repos().await?)?),
            "github_branches" => Ok(serde_json::to_value(self.github()?.branches(str_arg(args, "repo")?).await?)?),
            "github_head" => Ok(serde_json::to_value(self.github()?.head(str_arg(args, "repo")?, str_arg(args, "branch")?).await?)?),
            "site_links" => {
                let server = self.server(str_arg(args, "server")?)?;
                let snap = self.snapshot(&server)?;
                let mut out = serde_json::Map::new();
                for site in &snap.sites {
                    let Some(d) = site.domains.first() else { continue };
                    if let Some(l) = self.store.site_link(server.id, d)? {
                        out.insert(d.clone(), json!({ "repo": l.repo, "branch": l.branch, "app_path": l.app_path }));
                    }
                }
                Ok(Value::Object(out))
            }
            "cloudflare_accounts" => Ok(serde_json::to_value(self.cloudflare()?.accounts().await?)?),
            "files_list" | "files_read" => {
                let (server, root) = self.site_root(args)?;
                let files = sesame_ssh::files::SiteFiles::open(&server, &root).await?;
                let path = opt_str(args, "path").unwrap_or("/");
                if name == "files_list" {
                    let list = files.list(path).await?;
                    Ok(json!({
                        "root": root,
                        "owner": files.owner(),
                        "entries": list.iter().map(|e| json!({ "name": e.name, "dir": e.is_dir, "link": e.is_link, "size": e.size, "modified": e.modified, "mode": format!("{:o}", e.mode) })).collect::<Vec<_>>(),
                    }))
                } else {
                    let data = files.read(path, 1024 * 1024).await?;
                    match String::from_utf8(data) {
                        Ok(text) => Ok(json!({ "path": path, "text": text })),
                        Err(_) => bail!("{path} is not a text file"),
                    }
                }
            }
            "cloudflare_zones" => {
                let cf = self.cloudflare()?;
                Ok(serde_json::to_value(cf.zones().await?)?)
            }
            "cloudflare_records" => {
                let cf = self.cloudflare()?;
                let zone = self.zone_id(&cf, str_arg(args, "zone")?).await?;
                Ok(serde_json::to_value(cf.records(&zone).await?)?)
            }
            _ => bail!("`{name}` is not a read tool"),
        }
    }

    fn cloudflare(&self) -> Result<sesame_cloudflare::Client> {
        let token = secrets::get_global("CLOUDFLARE_TOKEN")?.context("Cloudflare is not connected in Sesame (Settings → Integrations)")?;
        sesame_cloudflare::Client::new(token)
    }

    fn github(&self) -> Result<sesame_github::Client> {
        let token = secrets::get_global("GITHUB_TOKEN")?.context("GitHub is not connected in Sesame (Settings → Integrations)")?;
        sesame_github::Client::new(token)
    }

    /// The server and the site directory named by `server` + `domain`.
    fn site_root(&self, args: &Value) -> Result<(ServerConfig, String)> {
        let server = self.server(str_arg(args, "server")?)?;
        let root = site_app(&self.snapshot(&server)?, str_arg(args, "domain")?)?;
        Ok((server, root))
    }

    fn resend(&self) -> Result<sesame_mail::Client> {
        let token = secrets::get_global("RESEND_TOKEN")?.context("Resend is not connected in Sesame (Settings → Integrations)")?;
        sesame_mail::Client::new(token)
    }

    async fn zone_id(&self, cf: &sesame_cloudflare::Client, zone: &str) -> Result<String> {
        let zones = cf.zones().await?;
        zones
            .iter()
            .find(|z| z.id == zone || z.name == zone)
            .map(|z| z.id.clone())
            .with_context(|| format!("no Cloudflare zone `{zone}`"))
    }

    /// Builds the plan for a change tool. Cloudflare changes become a plan
    /// with no server steps and are run by [`Engine::execute`] specially.
    async fn plan(&self, name: &str, args: &Value) -> Result<(Option<ServerConfig>, Plan)> {
        use sesame_ops::actions;
        if let Some(plan) = self.direct_plan(name, args)? {
            let server = opt_str(args, "server").map(|n| self.server(n)).transpose()?;
            return Ok((server, plan));
        }
        if name.starts_with("cloudflare_") {
            return Ok((None, cloudflare_plan(name, args)?));
        }
        let server = self.server(str_arg(args, "server")?)?;
        let snap = self.store.snapshot(server.id)?;
        let need_snap = || snap.clone().context("discover the server first (discover_server)");
        let plan = match name {
            "prepare_server" => {
                let o = sesame_provision::stack::StackOptions {
                    web: if opt_str(args, "web") == Some("nginx") { sesame_provision::stack::Web::Nginx } else { sesame_provision::stack::Web::Caddy },
                    php: opt_str(args, "php").unwrap_or("8.4").to_string(),
                    firewall: bool_arg(args, "firewall", true),
                };
                sesame_provision::stack::prepare_server(&o, &snap.clone().unwrap_or_default())?
            }
            "service_action" => {
                let kind = match str_arg(args, "kind")? {
                    "systemd" => actions::ServiceKind::Systemd,
                    "supervisor" => actions::ServiceKind::Supervisor,
                    k => bail!("kind must be systemd or supervisor, not {k}"),
                };
                let action = match str_arg(args, "action")? {
                    "start" => actions::ServiceAction::Start,
                    "stop" => actions::ServiceAction::Stop,
                    "restart" => actions::ServiceAction::Restart,
                    "enable" => actions::ServiceAction::Enable,
                    "disable" => actions::ServiceAction::Disable,
                    a => bail!("unknown action {a}"),
                };
                actions::service(kind, str_arg(args, "name")?, action)?
            }
            "install_service" | "uninstall_service" => {
                let svc = str_arg(args, "service")?;
                let panel = sesame_provision::Panel::ALL.into_iter().find(|p| p.dir() == svc);
                let service = sesame_provision::Service::ALL.into_iter().find(|s| s.slug() == svc || (svc == "postgres" && *s == sesame_provision::Service::Postgres));
                match (name, panel, service) {
                    ("install_service", Some(p), _) => sesame_provision::install_panel(p, &need_snap()?)?,
                    ("install_service", None, Some(s)) => sesame_provision::install(s, &need_snap()?)?,
                    ("uninstall_service", Some(p), _) => sesame_provision::uninstall_panel(p)?,
                    ("uninstall_service", None, Some(s)) => sesame_provision::uninstall(s, !bool_arg(args, "delete_data", false))?,
                    _ => bail!("unknown service `{svc}`"),
                }
            }
            "create_database" => {
                let kind = engine_kind(str_arg(args, "engine")?)?;
                let db = str_arg(args, "name")?;
                actions::db_create(kind, db, opt_str(args, "user").unwrap_or(db), self.mongo_admin(&server).as_ref())?
            }
            "drop_database" => {
                let kind = engine_kind(str_arg(args, "engine")?)?;
                actions::db_drop(kind, str_arg(args, "name")?, opt_str(args, "user"), self.mongo_admin(&server).as_ref())?
            }
            "create_site" => {
                use sesame_ops::site::{NewSite, Runtime, SiteKind, Source};
                let kind = match opt_str(args, "kind").unwrap_or("laravel") {
                    "laravel" => SiteKind::Laravel,
                    "static" => SiteKind::Static,
                    k => bail!("kind must be laravel or static, not {k}"),
                };
                let domain = str_arg(args, "domain")?.to_lowercase();
                let site = NewSite {
                    domain: domain.clone(),
                    aliases: vec![],
                    kind,
                    runtime: if opt_str(args, "runtime") == Some("octane") { Runtime::Octane } else { Runtime::Fpm },
                    source: opt_str(args, "repo").map(|r| Source { repo: r.to_string(), branch: opt_str(args, "branch").unwrap_or("main").to_string() }),
                    web_root: opt_str(args, "web_root").map(String::from).unwrap_or_else(|| if kind == SiteKind::Laravel { "public".into() } else { ".".into() }),
                    database: opt_str(args, "database").map(engine_kind).transpose()?,
                    scheduler: kind == SiteKind::Laravel && bool_arg(args, "scheduler", true),
                };
                let mut plan = sesame_ops::site::new_site(&site, &need_snap()?)?;
                let (_, app) = sesame_ops::site::site_paths(&domain);
                if kind == SiteKind::Laravel && bool_arg(args, "redis", false) {
                    let admin = secrets::get_named(server.id, "REDIS_PASSWORD").ok().flatten();
                    let r = sesame_ops::site::redis_for_site(&domain, &app, admin.as_deref())?;
                    plan.steps.extend(r.steps);
                    plan.details.extend(r.details);
                    plan.reveal.extend(r.reveal);
                }
                if kind == SiteKind::Laravel && bool_arg(args, "queue", false) {
                    let conn = if bool_arg(args, "redis", false) { "redis" } else { "database" };
                    let q = sesame_ops::site::queue_for_site(&domain, &app, conn, 1)?;
                    plan.steps.extend(q.steps);
                    plan.details.extend(q.details);
                }
                plan
            }
            "remove_site" => {
                let db = opt_str(args, "database").map(engine_kind).transpose()?;
                sesame_ops::site::remove_site(str_arg(args, "domain")?, db, &need_snap()?)?
            }
            "site_maintenance" => {
                let app = site_app(&need_snap()?, str_arg(args, "domain")?)?;
                actions::maintenance(&app, bool_arg(args, "on", true))?
            }
            "site_redis" | "site_queue" | "site_scheduler" | "site_database" => {
                let domain = str_arg(args, "domain")?;
                let snap = need_snap()?;
                let app = site_app(&snap, domain)?;
                match name {
                    "site_redis" => {
                        let admin = secrets::get_named(server.id, "REDIS_PASSWORD").ok().flatten();
                        sesame_ops::site::redis_for_site(domain, &app, admin.as_deref())?
                    }
                    "site_queue" => sesame_ops::site::queue_for_site(domain, &app, opt_str(args, "connection").unwrap_or("redis"), 1)?,
                    "site_scheduler" => sesame_ops::site::scheduler_for_site(domain, &app)?,
                    _ => sesame_ops::site::database_for_site(domain, &app, engine_kind(str_arg(args, "engine")?)?, true)?,
                }
            }
            "deploy_site" => {
                let domain = str_arg(args, "domain")?;
                let link = self.store.site_link(server.id, domain)?.with_context(|| format!("{domain} is not linked to a GitHub repository yet (do it in the app)"))?;
                let snap = need_snap()?;
                let site = snap.sites.iter().find(|s| s.domains.iter().any(|d| d == domain));
                let laravel = site.is_some_and(|s| s.framework == sesame_core::snapshot::Framework::Laravel);
                let opts = actions::DeployOptions { public_https: false, laravel, migrate: laravel, restart: vec![] };
                actions::deploy(&link.app_path, &link.repo, &link.branch, &opts)?
            }
            "site_reverb" => {
                let domain = str_arg(args, "domain")?;
                let snap = need_snap()?;
                let site = snap.sites.iter().find(|s| s.domains.iter().any(|d| d == domain)).with_context(|| format!("no site {domain}"))?;
                let port = match args.get("port").and_then(Value::as_u64) {
                    Some(p) => p as u16,
                    None => (8080u16..8200).find(|p| !snap.ports.iter().any(|x| x.port == *p)).unwrap_or(8080),
                };
                sesame_provision::reverb_for_site(site, port)?
            }
            "site_enabled" => sesame_ops::site::set_site_enabled(str_arg(args, "domain")?, bool_arg(args, "enabled", true))?,
            "site_access_grant" => {
                let domain = str_arg(args, "domain")?;
                let app = site_app(&need_snap()?, domain)?;
                sesame_ops::site::grant_access(domain, &app, str_arg(args, "person")?, str_arg(args, "public_key")?, bool_arg(args, "shell", false))?
            }
            "site_access_revoke" => {
                let domain = str_arg(args, "domain")?;
                let app = site_app(&need_snap()?, domain)?;
                sesame_ops::site::revoke_access(domain, &app, str_arg(args, "person")?)?
            }
            "site_mail_resend" => {
                let domain = str_arg(args, "domain")?.to_lowercase();
                let snap = need_snap()?;
                let site = snap.sites.iter().find(|s| s.domains.iter().any(|d| *d == domain)).with_context(|| format!("no site {domain}"))?;
                let laravel = site.framework == sesame_core::snapshot::Framework::Laravel;
                let mut details = vec![
                    format!("add {domain} to Resend (EU region)"),
                    "publish DKIM, SPF and the bounce MX in Cloudflare (DNS only; another provider's SPF is kept)".into(),
                    "ask Resend to verify".into(),
                ];
                if laravel {
                    details.push("a sending key limited to this domain, written as MAIL_* to .env".into());
                }
                Plan {
                    op: "site_mail_resend",
                    title: format!("mail for {domain} via Resend"),
                    risk: Risk::Change,
                    details,
                    confirm_word: None,
                    steps: vec![sesame_ops::Step::new("resend", serde_json::to_string(&json!({ "domain": domain, "app": site.app_path.clone().filter(|_| laravel) }))?)],
                    reveal: vec![],
                    secret_scope: Some(domain.clone()),
                }
            }
            "run_command" => {
                let command = str_arg(args, "command")?;
                Plan {
                    op: "run_command",
                    title: format!("run: {}", command.chars().take(80).collect::<String>()),
                    risk: Risk::Destructive,
                    details: vec![format!("runs as root: {command}")],
                    confirm_word: None,
                    steps: vec![sesame_ops::Step::new("command", command.to_string())],
                    reveal: vec![],
                    secret_scope: None,
                }
            }
            _ => bail!("`{name}` is not a change tool"),
        };
        Ok((Some(server), plan))
    }

    fn mongo_admin(&self, server: &ServerConfig) -> Option<sesame_ops::actions::Admin> {
        let get = |k: &str| secrets::get_named(server.id, k).ok().flatten();
        Some(sesame_ops::actions::Admin { user: get("MONGO_ADMIN_USER")?, password: get("MONGO_ADMIN_PASSWORD")? })
    }

    /// Asks the user in the app (for agents) and waits for the answer.
    async fn consent(&self, actor: &Actor, server: Option<&ServerConfig>, plan: &Plan, consent: Consent) -> Result<()> {
        let Consent::AskInApp { timeout } = consent else { return Ok(()) };
        let id = uuid::Uuid::new_v4().to_string();
        self.store.add_approval(&Approval {
            id: id.clone(),
            created_at: now(),
            actor: actor.0.clone(),
            server_id: server.map(|s| s.id).unwrap_or_default(),
            title: plan.title.clone(),
            details: plan.details.clone(),
            risk: if plan.risk == Risk::Destructive { "destructive".into() } else { "change".into() },
            status: "pending".into(),
        })?;
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            match self.store.approval_status(&id)?.as_deref() {
                Some("approved") => return Ok(()),
                Some("denied") => bail!("the user declined “{}” in Sesame", plan.title),
                _ => {}
            }
            if tokio::time::Instant::now() >= deadline {
                let _ = self.store.decide_approval(&id, "expired");
                bail!("no answer in Sesame within {} s — nothing was changed", timeout.as_secs());
            }
            tokio::time::sleep(Duration::from_millis(700)).await;
        }
    }

    async fn execute(&self, actor: &Actor, server: Option<ServerConfig>, plan: Plan, consent: Consent) -> Result<Value> {
        self.consent(actor, server.as_ref(), &plan, consent).await?;
        let result: Result<(String, Vec<(String, String)>)> = match &server {
            _ if plan.op == "direct" => self.run_direct(server.as_ref(), &plan).await,
            None => run_cloudflare(&self.cloudflare()?, &plan).await.map(|tail| (tail, vec![])),
            Some(server) if plan.op == "site_mail_resend" => self.run_resend(server, &plan).await,
            Some(server) => async {
                let session = sesame_ssh::Session::connect(server).await?;
                let out = sesame_ops::execute(&session, &plan, None).await;
                let _ = session.close().await;
                let out = out?;
                Ok((out.tail, out.reveal))
            }
            .await,
        };
        let (ok, log) = match &result {
            Ok((tail, _)) => (true, tail.clone()),
            Err(e) => (false, format!("{e:#}")),
        };
        self.store.log_audit(&AuditEntry {
            at: now(),
            server_id: server.as_ref().map(|s| s.id).unwrap_or_default(),
            op: plan.op.to_string(),
            title: plan.title.clone(),
            actor: actor.0.clone(),
            ok,
            log,
        })?;
        let (tail, reveal) = result?;
        let mut stored = Vec::new();
        if let Some(server) = &server {
            for (k, v) in &reveal {
                let name = plan.secret_name(k);
                secrets::set_named(server.id, &name, v)?;
                stored.push(name);
            }
            // Keep the app's picture current.
            if let Ok((snap, _)) = sesame_discovery::discover_server(server).await {
                let _ = self.store.save_snapshot(server.id, &snap);
            }
        }
        let share = self.store.setting_or("agents.share_secrets", "false") == "true" || !actor.is_agent();
        Ok(json!({
            "ok": true,
            "title": plan.title,
            "output": tail.lines().rev().take(40).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join("\n"),
            "secrets_stored_in_vault": stored,
            "secrets": if share { json!(reveal.into_iter().collect::<std::collections::BTreeMap<_, _>>()) } else { json!("hidden from agents; enable Settings → Agents → share secrets to return them") },
        }))
    }
}

impl Engine {
    /// Tools that are API calls or Sesame's own state rather than a server
    /// script. Their arguments ride in the plan's single step and are run by
    /// [`Engine::run_direct`] after consent. Titles never contain secrets.
    fn direct_plan(&self, name: &str, args: &Value) -> Result<Option<Plan>> {
        let (title, details, risk) = match name {
            "add_server" => (format!("add server {} ({})", str_arg(args, "name")?, str_arg(args, "target")?), vec![], Risk::Change),
            "remove_server" => (format!("remove {} from Sesame", str_arg(args, "server")?), vec!["its saved secrets are deleted; the server is not touched".into()], Risk::Destructive),
            "set_integration" => (format!("connect {}", str_arg(args, "kind")?), vec!["the token is checked, then kept in the encrypted vault".into()], Risk::Change),
            "remove_integration" => (format!("disconnect {}", str_arg(args, "kind")?), vec![], Risk::Destructive),
            "github_link_site" => (
                format!("link {} to {}@{}", str_arg(args, "domain")?, str_arg(args, "repo")?, str_arg(args, "branch")?),
                vec!["a read-only deploy key is made on the server and added to the repository".into()],
                Risk::Change,
            ),
            "cloudflare_create_zone" => (format!("add {} to Cloudflare", str_arg(args, "name")?), vec![], Risk::Change),
            "files_write" => (format!("save {} on {}", str_arg(args, "path")?, str_arg(args, "domain")?), vec![], Risk::Change),
            "files_mkdir" => (format!("create folder {} on {}", str_arg(args, "path")?, str_arg(args, "domain")?), vec![], Risk::Change),
            "files_rename" => (format!("rename {} → {} on {}", str_arg(args, "from")?, str_arg(args, "to")?, str_arg(args, "domain")?), vec![], Risk::Change),
            "files_remove" => (format!("delete {} on {}", str_arg(args, "path")?, str_arg(args, "domain")?), vec![], Risk::Destructive),
            _ => return Ok(None),
        };
        Ok(Some(Plan {
            op: "direct",
            title,
            risk,
            details,
            confirm_word: None,
            steps: vec![sesame_ops::Step::new(name.to_string(), serde_json::to_string(args)?)],
            reveal: vec![],
            secret_scope: None,
        }))
    }

    async fn run_direct(&self, server: Option<&ServerConfig>, plan: &Plan) -> Result<(String, Vec<(String, String)>)> {
        let step = plan.steps.first().ok_or_else(|| anyhow!("empty plan"))?;
        let args: Value = serde_json::from_str(&step.script)?;
        let args = &args;
        let need_server = || server.cloned().context("missing server");
        let out = match step.title.as_str() {
            "add_server" => {
                let name = str_arg(args, "name")?;
                if self.store.servers()?.iter().any(|s| s.name == name) {
                    bail!("a server named {name} already exists");
                }
                let target = str_arg(args, "target")?;
                let (user, rest) = target.split_once('@').unwrap_or(("root", target));
                let (host, port) = match rest.rsplit_once(':') {
                    Some((h, p)) => (h, p.parse().context("bad port")?),
                    None => (rest, 22),
                };
                if host.is_empty() || user.is_empty() {
                    bail!("expected user@host[:port]");
                }
                let mut s = ServerConfig::new(name, host, user);
                s.port = port;
                if let Some(path) = opt_str(args, "key_path") {
                    s.auth = sesame_core::AuthMethod::KeyFile { path: path.to_string() };
                } else if let Some(pw) = opt_str(args, "password") {
                    s.auth = sesame_core::AuthMethod::Password;
                    secrets::set(s.id, secrets::SecretKind::Password, pw)?;
                }
                self.store.save_server(&s)?;
                // Discover right away so the server is useful and the host key is learned.
                match sesame_discovery::discover_server(&s).await {
                    Ok((snap, _)) => {
                        self.store.save_snapshot(s.id, &snap)?;
                        format!("{} saved and discovered: {} sites", s.name, snap.sites.len())
                    }
                    Err(e) => format!("{} saved; discovery failed: {e:#}", s.name),
                }
            }
            "remove_server" => {
                let s = need_server()?;
                let _ = secrets::delete_server(s.id);
                self.store.delete_server(s.id)?;
                format!("{} removed from Sesame", s.name)
            }
            "set_integration" | "remove_integration" => {
                let key = match str_arg(args, "kind")? {
                    "github" => "GITHUB_TOKEN",
                    "cloudflare" => "CLOUDFLARE_TOKEN",
                    "resend" => "RESEND_TOKEN",
                    k => bail!("unknown integration {k}"),
                };
                if step.title == "remove_integration" {
                    secrets::delete_global(key)?;
                    "disconnected".into()
                } else {
                    let token = str_arg(args, "token")?.trim().to_string();
                    match key {
                        "GITHUB_TOKEN" => drop(sesame_github::Client::new(token.clone())?.user().await?),
                        "CLOUDFLARE_TOKEN" => sesame_cloudflare::Client::new(token.clone())?.verify().await?,
                        _ => sesame_mail::Client::new(token.clone())?.check().await?,
                    }
                    secrets::set_global(key, &token)?;
                    "connected".into()
                }
            }
            "github_link_site" => {
                let server = need_server()?;
                let domain = str_arg(args, "domain")?;
                let (repo, branch) = (str_arg(args, "repo")?, str_arg(args, "branch")?);
                let app = site_app(&self.snapshot(&server)?, domain)?;
                let gh = self.github()?;
                gh.branches(repo).await?.iter().find(|b| b.name == branch).with_context(|| format!("{repo} has no branch {branch}"))?;
                let key_plan = sesame_ops::actions::git_connect(&app)?;
                let session = sesame_ssh::Session::connect(&server).await?;
                let out = sesame_ops::execute(&session, &key_plan, None).await;
                let _ = session.close().await;
                let out = out?;
                let public = out.reveal.iter().find(|(k, _)| k == "DEPLOY_PUBLIC_KEY").map(|(_, v)| v.clone()).context("no deploy key was made")?;
                gh.ensure_deploy_key(repo, &format!("sesame · {domain}"), &public).await?;
                self.store.set_site_link(&sesame_core::SiteLink {
                    server_id: server.id,
                    domain: domain.to_string(),
                    app_path: app,
                    repo: repo.to_string(),
                    branch: branch.to_string(),
                })?;
                format!("{domain} is linked to {repo}@{branch}; deploy key added")
            }
            "cloudflare_create_zone" => {
                let cf = self.cloudflare()?;
                let account = match opt_str(args, "account_id") {
                    Some(a) => a.to_string(),
                    None => cf.accounts().await?.first().map(|a| a.id.clone()).context("the token sees no Cloudflare account")?,
                };
                let z = cf.create_zone(&account, str_arg(args, "name")?).await?;
                format!("{} added ({}); set these nameservers at your registrar: {}", z.name, z.status, z.name_servers.join(", "))
            }
            "files_write" | "files_mkdir" | "files_rename" | "files_remove" => {
                let (server, root) = self.site_root(args)?;
                let files = sesame_ssh::files::SiteFiles::open(&server, &root).await?;
                match step.title.as_str() {
                    "files_write" => files.write(str_arg(args, "path")?, str_arg(args, "content")?.as_bytes()).await?,
                    "files_mkdir" => files.mkdir(str_arg(args, "path")?).await?,
                    "files_rename" => files.rename(str_arg(args, "from")?, str_arg(args, "to")?).await?,
                    _ => files.remove(str_arg(args, "path")?).await?,
                }
                "done".into()
            }
            t => bail!("unknown tool {t}"),
        };
        Ok((out, vec![]))
    }

    /// Resend + Cloudflare, then MAIL_* on the server for a Laravel site.
    async fn run_resend(&self, server: &ServerConfig, plan: &Plan) -> Result<(String, Vec<(String, String)>)> {
        let spec: Value = serde_json::from_str(&plan.steps[0].script)?;
        let domain = str_arg(&spec, "domain")?;
        let rs = self.resend()?;
        let cf = self.cloudflare()?;
        let zones = cf.zones().await?;
        let zone = sesame_cloudflare::zone_for(domain, &zones).with_context(|| format!("no Cloudflare zone for {domain}"))?.clone();
        let mut lines = Vec::new();
        let d = match rs.domains().await?.into_iter().find(|d| d.name == domain) {
            Some(d) => rs.domain(&d.id).await?,
            None => rs.create_domain(domain, "eu-west-1").await?,
        };
        for change in sesame_mail::diff(&sesame_mail::cloudflare_records(&d), &cf.records(&zone.id).await?) {
            match change {
                sesame_mail::Change::Create(r) => lines.push(format!("{} {} added", cf.create_record(&zone.id, &r).await?.kind, r.name)),
                sesame_mail::Change::Update(r) => lines.push(format!("{} {} updated", cf.update_record(&zone.id, &r).await?.kind, r.name)),
                sesame_mail::Change::Same => {}
            }
        }
        rs.verify(&d.id).await?;
        lines.push("verification requested".into());
        let mut reveal = Vec::new();
        if let Some(app) = spec.get("app").and_then(Value::as_str) {
            let key = rs.sending_key(&format!("sesame {domain}"), &d.id).await?;
            let smtp = sesame_ops::site::Smtp {
                host: sesame_mail::SMTP_HOST.into(),
                port: sesame_mail::SMTP_PORT,
                username: sesame_mail::SMTP_USER.into(),
                password: key,
                from: format!("hello@{domain}"),
            };
            let env_plan = sesame_ops::site::mail_for_site(domain, app, &smtp)?;
            let session = sesame_ssh::Session::connect(server).await?;
            let out = sesame_ops::execute(&session, &env_plan, None).await;
            let _ = session.close().await;
            let out = out?;
            lines.push(out.tail);
            reveal = out.reveal;
        }
        Ok((lines.join("\n"), reveal))
    }
}

fn site_app(snap: &sesame_core::ServerSnapshot, domain: &str) -> Result<String> {
    snap.sites
        .iter()
        .find(|s| s.domains.iter().any(|d| d == domain))
        .and_then(|s| s.app_path.clone())
        .with_context(|| format!("no site {domain} in the last discovery"))
}

fn summary(s: &sesame_core::ServerSnapshot) -> Value {
    json!({
        "taken_at": s.taken_at,
        "os": s.os,
        "web_servers": s.web_servers,
        "sites": s.sites.iter().map(|x| json!({ "domains": x.domains, "framework": x.framework, "app_path": x.app_path })).collect::<Vec<_>>(),
        "databases": s.databases.iter().map(|d| json!({ "kind": d.kind, "version": d.version, "databases": d.databases })).collect::<Vec<_>>(),
        "processes": s.supervisor.iter().map(|p| json!({ "name": p.name, "state": p.state, "role": p.role })).collect::<Vec<_>>(),
        "php": s.php,
        "insights": sesame_discovery::insights::insights(s, time::OffsetDateTime::now_utc()),
        "warnings": s.warnings,
    })
}

fn cloudflare_plan(name: &str, args: &Value) -> Result<Plan> {
    let zone = str_arg(args, "zone")?;
    let (title, details, risk) = match name {
        "cloudflare_upsert_record" => {
            let r = format!(
                "{} {} → {}{}",
                str_arg(args, "type")?,
                str_arg(args, "name")?,
                str_arg(args, "content")?,
                if bool_arg(args, "proxied", false) { " (proxied)" } else { "" }
            );
            (format!("DNS {r}"), vec![format!("zone {zone}: {r}")], Risk::Change)
        }
        "cloudflare_delete_record" => (format!("delete DNS record {}", str_arg(args, "name")?), vec![format!("zone {zone}")], Risk::Destructive),
        "cloudflare_purge" => (format!("purge Cloudflare cache of {zone}"), vec!["every cached file of the zone".into()], Risk::Change),
        _ => bail!("unknown Cloudflare tool {name}"),
    };
    Ok(Plan {
        op: "cloudflare",
        title,
        risk,
        details,
        confirm_word: None,
        steps: vec![sesame_ops::Step::new("cloudflare", serde_json::to_string(&json!({ "tool": name, "args": args }))?)],
        reveal: vec![],
        secret_scope: None,
    })
}

async fn run_cloudflare(cf: &sesame_cloudflare::Client, plan: &Plan) -> Result<String> {
    let spec: Value = serde_json::from_str(&plan.steps.first().ok_or_else(|| anyhow!("empty plan"))?.script)?;
    let args = &spec["args"];
    let zones = cf.zones().await?;
    let zone = str_arg(args, "zone")?;
    let zone_id = zones.iter().find(|z| z.id == zone || z.name == zone).map(|z| z.id.clone()).with_context(|| format!("no zone {zone}"))?;
    match spec["tool"].as_str().unwrap_or("") {
        "cloudflare_upsert_record" => {
            let name = str_arg(args, "name")?.to_lowercase();
            let kind = str_arg(args, "type")?.to_uppercase();
            let existing = cf.records(&zone_id).await?.into_iter().find(|r| r.name == name && r.kind == kind);
            let rec = sesame_cloudflare::DnsRecord {
                id: existing.map(|r| r.id).unwrap_or_default(),
                kind,
                name,
                content: str_arg(args, "content")?.to_string(),
                proxied: bool_arg(args, "proxied", false),
                ttl: args.get("ttl").and_then(Value::as_u64).unwrap_or(1) as u32,
                priority: args.get("priority").and_then(Value::as_u64).map(|p| p as u16),
            };
            let saved = if rec.id.is_empty() { cf.create_record(&zone_id, &rec).await? } else { cf.update_record(&zone_id, &rec).await? };
            Ok(format!("{} {} → {}", saved.kind, saved.name, saved.content))
        }
        "cloudflare_delete_record" => {
            let name = str_arg(args, "name")?.to_lowercase();
            let kind = opt_str(args, "type").map(str::to_uppercase);
            let rec = cf
                .records(&zone_id)
                .await?
                .into_iter()
                .find(|r| r.name == name && kind.as_ref().is_none_or(|k| &r.kind == k))
                .with_context(|| format!("no record {name}"))?;
            cf.delete_record(&zone_id, &rec.id).await?;
            Ok(format!("deleted {} {}", rec.kind, rec.name))
        }
        "cloudflare_purge" => {
            cf.purge_everything(&zone_id).await?;
            Ok("cache purged".into())
        }
        t => bail!("unknown Cloudflare tool {t}"),
    }
}
