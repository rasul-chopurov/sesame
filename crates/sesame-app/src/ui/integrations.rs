//! GitHub and Cloudflare: connecting tokens, loading data, the Cloudflare
//! page, and the Repository and DNS cards on a site.

use std::collections::HashMap;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use sesame_cloudflare::{Account, DnsRecord, DnsStatus, Zone};
use sesame_core::ops::Risk;
use sesame_core::snapshot::{Framework, ProcessRole, ServerSnapshot, Site};
use sesame_core::{AuditEntry, ServerId, SiteLink, Store as _, secrets};
use sesame_github::{Commit, Repo};
use sesame_ops::Plan;

use super::jobs::{After, Job, JobState};
use super::theme::Glass;
use super::widgets::*;
use super::{Page, SesameApp};
use crate::i18n::{tr, trp};
use crate::runtime;

pub const GITHUB_TOKEN: &str = "GITHUB_TOKEN";
pub const CLOUDFLARE_TOKEN: &str = "CLOUDFLARE_TOKEN";

#[derive(Default)]
pub struct GhState {
    pub login: Option<String>,
    pub repos: Vec<Repo>,
    pub loading: bool,
    pub error: Option<String>,
    /// `repo@branch` → latest commit on GitHub.
    pub heads: HashMap<String, Commit>,
    /// Branches of the repository being linked.
    pub branches: HashMap<String, Vec<String>>,
}

#[derive(Default)]
pub struct CfState {
    pub connected: bool,
    pub accounts: Vec<Account>,
    pub zones: Vec<Zone>,
    pub records: Vec<(String, Vec<DnsRecord>)>,
    pub loading: bool,
    pub error: Option<String>,
    pub selected: Option<String>,
    /// Record table filter by type (None = all).
    pub filter_type: Option<String>,
    /// The add-record form.
    pub add_type: String,
    pub add_ttl: u32,
    pub add_proxied: bool,
}

impl CfState {
    pub fn fresh() -> Self {
        Self { add_type: "A".into(), add_ttl: 1, add_proxied: true, ..Default::default() }
    }
}

/// A Cloudflare change, run after confirmation.
#[derive(Clone, Debug)]
pub enum ApiAction {
    Upsert { zone_id: String, records: Vec<DnsRecord> },
    Delete { zone_id: String, record_id: String },
    CreateZone { account_id: String, name: String },
    Purge { zone_id: String },
}

pub struct Inputs {
    pub gh_token: Entity<InputState>,
    pub cf_token: Entity<InputState>,
    pub repo_search: Entity<InputState>,
    pub new_zone: Entity<InputState>,
    pub rec_name: Entity<InputState>,
    pub rec_content: Entity<InputState>,
    pub rec_priority: Entity<InputState>,
    pub zone_search: Entity<InputState>,
    pub record_search: Entity<InputState>,
}

impl Inputs {
    pub fn new(window: &mut Window, cx: &mut Context<SesameApp>) -> Self {
        let masked = |window: &mut Window, cx: &mut Context<SesameApp>| cx.new(|cx| InputState::new(window, cx).masked(true));
        let plain = |ph: String, window: &mut Window, cx: &mut Context<SesameApp>| cx.new(|cx| InputState::new(window, cx).placeholder(ph));
        Self {
            gh_token: masked(window, cx),
            cf_token: masked(window, cx),
            repo_search: plain(tr("repo.search"), window, cx),
            new_zone: plain("example.com".into(), window, cx),
            rec_name: plain("www".into(), window, cx),
            rec_content: plain("203.0.113.10".into(), window, cx),
            rec_priority: plain("10".into(), window, cx),
            zone_search: plain(tr("cf.search_domains"), window, cx),
            record_search: plain(tr("cf.search_records"), window, cx),
        }
    }

    pub fn all(&self) -> [&Entity<InputState>; 9] {
        [
            &self.gh_token,
            &self.cf_token,
            &self.repo_search,
            &self.new_zone,
            &self.rec_name,
            &self.rec_content,
            &self.rec_priority,
            &self.zone_search,
            &self.record_search,
        ]
    }
}

/// The server's public IP: the host itself, or its first resolved address.
pub fn server_ip(host: &str) -> Option<String> {
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Some(host.to_string());
    }
    use std::net::ToSocketAddrs;
    let addrs: Vec<_> = (host, 22).to_socket_addrs().ok()?.collect();
    addrs.iter().find(|a| a.is_ipv4()).or(addrs.first()).map(|a| a.ip().to_string())
}

impl SesameApp {
    // ---- connecting ---------------------------------------------------------

    pub(crate) fn load_integrations(&mut self, cx: &mut Context<Self>) {
        if secrets::get_global(GITHUB_TOKEN).ok().flatten().is_some() {
            self.load_github(cx);
        }
        if secrets::get_global(CLOUDFLARE_TOKEN).ok().flatten().is_some() {
            self.load_cloudflare(cx);
        }
    }

    pub(crate) fn connect_github(&mut self, cx: &mut Context<Self>) {
        let token = self.int.gh_token.read(cx).value().trim().to_string();
        if token.is_empty() {
            return;
        }
        if let Err(e) = secrets::set_global(GITHUB_TOKEN, &token) {
            self.gh.error = Some(e.to_string());
            return cx.notify();
        }
        self.load_github(cx);
    }

    pub(crate) fn disconnect_github(&mut self, cx: &mut Context<Self>) {
        let _ = secrets::delete_global(GITHUB_TOKEN);
        self.gh = GhState::default();
        cx.notify();
    }

    fn load_github(&mut self, cx: &mut Context<Self>) {
        let Some(token) = secrets::get_global(GITHUB_TOKEN).ok().flatten() else { return };
        self.gh.loading = true;
        self.gh.error = None;
        cx.notify();
        let job = runtime::get().spawn(async move {
            let gh = sesame_github::Client::new(token)?;
            let user = gh.user().await?;
            let repos = gh.repos().await?;
            anyhow::Ok((user, repos))
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |this, cx| {
                this.gh.loading = false;
                match result {
                    Ok(Ok((user, repos))) => {
                        this.gh.login = Some(user.login);
                        this.gh.repos = repos;
                    }
                    Ok(Err(e)) => this.gh.error = Some(format!("{e:#}")),
                    Err(e) => this.gh.error = Some(e.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn connect_cloudflare(&mut self, cx: &mut Context<Self>) {
        let token = self.int.cf_token.read(cx).value().trim().to_string();
        if token.is_empty() {
            return;
        }
        if let Err(e) = secrets::set_global(CLOUDFLARE_TOKEN, &token) {
            self.cf.error = Some(e.to_string());
            return cx.notify();
        }
        self.load_cloudflare(cx);
    }

    pub(crate) fn disconnect_cloudflare(&mut self, cx: &mut Context<Self>) {
        let _ = secrets::delete_global(CLOUDFLARE_TOKEN);
        self.cf = CfState::fresh();
        cx.notify();
    }

    pub(crate) fn load_cloudflare(&mut self, cx: &mut Context<Self>) {
        let Some(token) = secrets::get_global(CLOUDFLARE_TOKEN).ok().flatten() else { return };
        self.cf.loading = true;
        self.cf.error = None;
        cx.notify();
        let job = runtime::get().spawn(async move {
            let cf = sesame_cloudflare::Client::new(token)?;
            cf.verify().await?;
            let accounts = cf.accounts().await.unwrap_or_default();
            let zones = cf.zones().await?;
            let mut records = Vec::new();
            for z in &zones {
                records.push((z.id.clone(), cf.records(&z.id).await.unwrap_or_default()));
            }
            anyhow::Ok((accounts, zones, records))
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |this, cx| {
                this.cf.loading = false;
                match result {
                    Ok(Ok((accounts, zones, records))) => {
                        this.cf.connected = true;
                        this.cf.accounts = accounts;
                        this.cf.zones = zones;
                        this.cf.records = records;
                    }
                    Ok(Err(e)) => this.cf.error = Some(format!("{e:#}")),
                    Err(e) => this.cf.error = Some(e.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ---- Cloudflare changes ---------------------------------------------------

    pub(crate) fn request_api(&mut self, title: String, details: Vec<String>, risk: Risk, confirm: Option<String>, action: ApiAction, window: &mut Window, cx: &mut Context<Self>) {
        let server = self.servers.first().map(|s| s.id).unwrap_or_default();
        let plan = Plan { op: "cloudflare", title, risk, details, confirm_word: confirm, steps: vec![], reveal: vec![], secret_scope: None };
        self.request(server, Ok(plan), None, window, cx);
        if let Some(p) = self.pending.as_mut() {
            p.api = Some(action);
        }
    }

    pub(crate) fn run_api(&mut self, plan: Plan, action: ApiAction, cx: &mut Context<Self>) {
        let Some(token) = secrets::get_global(CLOUDFLARE_TOKEN).ok().flatten() else { return };
        self.job = Some(Job {
            server: ServerId::nil(),
            title: plan.title.clone(),
            steps: vec![plan.title.clone()],
            current: 0,
            lines: vec![],
            state: JobState::Running,
            reveal: vec![],
        });
        cx.notify();
        let task = action.clone();
        let job = runtime::get().spawn(async move {
            let cf = sesame_cloudflare::Client::new(token)?;
            let mut lines = Vec::new();
            match task {
                ApiAction::Upsert { zone_id, records } => {
                    for r in records {
                        let saved = if r.id.is_empty() { cf.create_record(&zone_id, &r).await? } else { cf.update_record(&zone_id, &r).await? };
                        lines.push(format!("{} {} → {}{}", saved.kind, saved.name, saved.content, if saved.proxied { " (proxied)" } else { "" }));
                    }
                }
                ApiAction::Delete { zone_id, record_id } => {
                    cf.delete_record(&zone_id, &record_id).await?;
                    lines.push("deleted".into());
                }
                ApiAction::CreateZone { account_id, name } => {
                    let z = cf.create_zone(&account_id, &name).await?;
                    lines.push(format!("{} added ({})", z.name, z.status));
                    lines.push(format!("nameservers: {}", z.name_servers.join(", ")));
                }
                ApiAction::Purge { zone_id } => {
                    cf.purge_everything(&zone_id).await?;
                    lines.push("cache purged".into());
                }
            }
            anyhow::Ok(lines)
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |this, cx| {
                let (ok, log, state, lines) = match result {
                    Ok(Ok(lines)) => (true, lines.join("\n"), JobState::Done, lines),
                    Ok(Err(e)) => (false, format!("{e:#}"), JobState::Failed(format!("{e:#}")), vec![]),
                    Err(e) => (false, e.to_string(), JobState::Failed(e.to_string()), vec![]),
                };
                let _ = this.store.log_audit(&AuditEntry {
                    at: sesame_discovery::parse::app::now_rfc3339(),
                    server_id: ServerId::nil(),
                    op: "cloudflare".into(),
                    title: plan.title.clone(),
                    actor: "user".into(),
                    ok,
                    log,
                });
                if let Some(job) = this.job.as_mut() {
                    job.lines = lines;
                    job.state = state;
                    job.current = 1;
                }
                if ok {
                    this.load_cloudflare(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn dns_status(&self, host: &str, ip: &str) -> Option<DnsStatus> {
        self.cf.connected.then(|| sesame_cloudflare::status(host, ip, &self.cf.zones, &self.cf.records))
    }

    /// Plan to point every given host at `ip`: create missing records, keep proxied.
    pub(crate) fn request_point_here(&mut self, hosts: Vec<String>, ip: String, window: &mut Window, cx: &mut Context<Self>) {
        let mut by_zone: HashMap<String, Vec<DnsRecord>> = HashMap::new();
        let mut details = Vec::new();
        for h in &hosts {
            match self.dns_status(h, &ip) {
                Some(DnsStatus::Missing { .. }) => {
                    if let Some(z) = sesame_cloudflare::zone_for(h, &self.cf.zones) {
                        details.push(format!("A {h} → {ip} (proxied)"));
                        by_zone.entry(z.id.clone()).or_default().push(sesame_cloudflare::record_for(h, &ip, true));
                    }
                }
                Some(DnsStatus::Elsewhere { record, .. }) => {
                    if let Some(z) = sesame_cloudflare::zone_for(h, &self.cf.zones) {
                        details.push(format!("{} {h}: {} → {ip}", record.kind, record.content));
                        let mut r = record.clone();
                        r.kind = if ip.contains(':') { "AAAA".into() } else { "A".into() };
                        r.content = ip.clone();
                        by_zone.entry(z.id.clone()).or_default().push(r);
                    }
                }
                _ => {}
            }
        }
        if by_zone.is_empty() {
            return;
        }
        // One confirmation per zone keeps the job simple; most sites live in one zone.
        let (zone_id, records) = by_zone.into_iter().next().unwrap();
        self.request_api(
            trp("dns.sync", &[("n", &records.len().to_string())]),
            details,
            Risk::Change,
            None,
            ApiAction::Upsert { zone_id, records },
            window,
            cx,
        );
    }

    // ---- GitHub ---------------------------------------------------------------

    pub(crate) fn load_branches(&mut self, repo: String, cx: &mut Context<Self>) {
        let Some(token) = secrets::get_global(GITHUB_TOKEN).ok().flatten() else { return };
        let r = repo.clone();
        let job = runtime::get().spawn(async move { sesame_github::Client::new(token)?.branches(&r).await });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(b)) => {
                        this.gh.branches.insert(repo, b.into_iter().map(|b| b.name).collect());
                    }
                    Ok(Err(e)) => this.gh.error = Some(format!("{e:#}")),
                    Err(e) => this.gh.error = Some(e.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn check_head(&mut self, repo: String, branch: String, cx: &mut Context<Self>) {
        let Some(token) = secrets::get_global(GITHUB_TOKEN).ok().flatten() else { return };
        let (r, b) = (repo.clone(), branch.clone());
        let job = runtime::get().spawn(async move { sesame_github::Client::new(token)?.head(&r, &b).await });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(c)) => {
                        this.gh.heads.insert(format!("{repo}@{branch}"), c);
                    }
                    Ok(Err(e)) => this.gh.error = Some(format!("{e:#}")),
                    Err(e) => this.gh.error = Some(e.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Creates the deploy key on the server; when the job finishes, the key is
    /// added to GitHub and the link is saved.
    pub(crate) fn request_link(&mut self, server: ServerId, site: &Site, repo: String, branch: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(app) = site.app_path.clone() else { return };
        let domain = site.domains.first().cloned().unwrap_or_default();
        let plan = sesame_ops::actions::git_connect(&app);
        self.request(server, plan, None, window, cx);
        if let Some(p) = self.pending.as_mut() {
            p.after = Some(After::DeployKey { link: SiteLink { server_id: server, domain, app_path: app, repo, branch } });
        }
    }

    pub(crate) fn finish_deploy_key(&mut self, link: SiteLink, public_key: String, cx: &mut Context<Self>) {
        let Some(token) = secrets::get_global(GITHUB_TOKEN).ok().flatten() else { return };
        let title = format!("sesame · {}", link.domain);
        let repo = link.repo.clone();
        let job = runtime::get().spawn(async move { sesame_github::Client::new(token)?.ensure_deploy_key(&repo, &title, &public_key).await });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |this, cx| {
                let line = match result {
                    Ok(Ok(())) => {
                        let _ = this.store.set_site_link(&link);
                        format!("deploy key added to {} — ready to deploy {}", link.repo, link.branch)
                    }
                    Ok(Err(e)) => format!("could not add the deploy key: {e:#}"),
                    Err(e) => e.to_string(),
                };
                if let Some(job) = this.job.as_mut() {
                    job.lines.push(line);
                    // The public key is not a secret worth revealing.
                    job.reveal.retain(|(k, _)| k != "DEPLOY_PUBLIC_KEY");
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn request_deploy(&mut self, server: ServerId, link: &SiteLink, snap: &ServerSnapshot, window: &mut Window, cx: &mut Context<Self>) {
        let site = snap.sites.iter().find(|s| s.domains.contains(&link.domain));
        let laravel = site.is_some_and(|s| s.framework == Framework::Laravel);
        let restart: Vec<String> = site
            .map(|s| {
                snap.supervisor
                    .iter()
                    .filter(|p| s.processes.contains(&p.name) && matches!(p.role, ProcessRole::Octane | ProcessRole::Horizon | ProcessRole::Reverb))
                    .map(|p| p.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        // Public repositories clone over HTTPS; private ones use the deploy key.
        let public_https = self.gh.repos.iter().any(|r| r.full_name == link.repo && !r.private);
        let opts = sesame_ops::actions::DeployOptions { public_https, laravel, migrate: laravel, restart };
        let plan = sesame_ops::actions::deploy(&link.app_path, &link.repo, &link.branch, &opts);
        self.request(server, plan, None, window, cx);
    }
}

// ---- views --------------------------------------------------------------------

pub fn settings_card(app: &SesameApp, cx: &mut Context<SesameApp>) -> Div {
    let g = Glass::get(cx).clone();
    let row = |name: &str, logo: &str, status: String, hint: String, input: &Entity<InputState>, connected: bool, loading: bool, error: Option<String>, connect: Box<dyn Fn(&mut SesameApp, &mut Context<SesameApp>)>, disconnect: Box<dyn Fn(&mut SesameApp, &mut Context<SesameApp>)>, cx: &mut Context<SesameApp>| {
        let connect = std::rc::Rc::new(connect);
        let disconnect = std::rc::Rc::new(disconnect);
        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .children(brand(logo, px(16.)))
                    .when(brand(logo, px(16.)).is_none(), |d| d.child(icon(IconName::Server, g.dim)))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(name.to_string()))
                    .child(pill(status, if connected { g.ok } else { g.faint })),
            )
            .child(div().text_xs().text_color(g.faint).child(hint))
            .child(
                h_flex()
                    .gap_2()
                    .child(div().w(px(380.)).child(Input::new(input)))
                    .child({
                        let c = connect.clone();
                        Button::new(SharedString::from(format!("connect-{name}")))
                            .primary()
                            .loading(loading)
                            .label(tr("int.connect"))
                            .on_click(cx.listener(move |this, _, _, cx| c(this, cx)))
                    })
                    .when(connected, |d| {
                        let dc = disconnect.clone();
                        d.child(
                            Button::new(SharedString::from(format!("disconnect-{name}")))
                                .ghost()
                                .label(tr("int.disconnect"))
                                .on_click(cx.listener(move |this, _, _, cx| dc(this, cx))),
                        )
                    }),
            )
            .when_some(error, |d, e| d.child(div().text_xs().text_color(g.bad).child(e)))
    };
    let gh_status = match &app.gh.login {
        Some(l) => trp("int.connected_as", &[("who", &format!("@{l}"))]),
        None => tr("int.not_connected"),
    };
    let cf_status = if app.cf.connected {
        trp(
            "int.connected_as",
            &[("who", &app.cf.accounts.first().map(|a| a.name.clone()).unwrap_or_else(|| format!("{} zones", app.cf.zones.len())))],
        )
    } else {
        tr("int.not_connected")
    };
    card(&tr("nav.integrations"), cx)
        .gap_4()
        .child(row(
            "GitHub",
            "github",
            gh_status,
            tr("int.github_hint"),
            &app.int.gh_token,
            app.gh.login.is_some(),
            app.gh.loading,
            app.gh.error.clone(),
            Box::new(|this, cx| this.connect_github(cx)),
            Box::new(|this, cx| this.disconnect_github(cx)),
            cx,
        ))
        .child(row(
            "Cloudflare",
            "cloudflare",
            cf_status,
            tr("int.cloudflare_hint"),
            &app.int.cf_token,
            app.cf.connected,
            app.cf.loading,
            app.cf.error.clone(),
            Box::new(|this, cx| this.connect_cloudflare(cx)),
            Box::new(|this, cx| this.disconnect_cloudflare(cx)),
            cx,
        ))
}

pub fn cloudflare_page(app: &SesameApp, cx: &mut Context<SesameApp>) -> impl IntoElement {
    let g = Glass::get(cx).clone();
    let zone = app.cf.selected.as_ref().and_then(|id| app.cf.zones.iter().find(|z| &z.id == id)).cloned();
    let header = h_flex()
        .px_5()
        .py_3()
        .gap_3()
        .border_b_1()
        .border_color(g.line)
        .children(brand("cloudflare", px(20.)))
        .child(
            h_flex()
                .flex_1()
                .gap_2()
                .child(
                    div()
                        .id("cf-home")
                        .text_lg()
                        .font_weight(FontWeight::BOLD)
                        .cursor_pointer()
                        .child("Cloudflare")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.cf.selected = None;
                            cx.notify();
                        })),
                )
                .when_some(zone.as_ref().map(|z| z.name.clone()), |d, name| {
                    d.child(div().text_lg().text_color(g.faint).child("/")).child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(name))
                }),
        )
        .child(
            Button::new("cf-refresh")
                .ghost()
                .loading(app.cf.loading)
                .icon(IconName::RefreshCw)
                .on_click(cx.listener(|this, _, _, cx| this.load_cloudflare(cx))),
        );
    if !app.cf.connected {
        let body = v_flex()
            .p_5()
            .gap_3()
            .when(app.cf.loading, |d| d.child(empty(tr("cf.loading"), cx)))
            .when(!app.cf.loading, |d| d.child(empty(trp("int.connect_first", &[("what", "Cloudflare")]), cx)))
            .when_some(app.cf.error.clone(), |d, e| d.child(div().text_color(g.bad).child(e)));
        return v_flex().size_full().child(header).child(body);
    }
    let body = match zone {
        None => zones_grid(app, cx).into_any_element(),
        Some(z) => zone_detail(app, &z, cx).into_any_element(),
    };
    v_flex()
        .size_full()
        .child(header)
        .child(div().id("cf-body").flex_1().min_h_0().overflow_y_scroll().p_5().child(body))
}

fn zones_grid(app: &SesameApp, cx: &mut Context<SesameApp>) -> Div {
    let g = Glass::get(cx).clone();
    let q = app.int.zone_search.read(cx).value().trim().to_lowercase();
    let mut grid = div().grid().grid_cols(3).gap_3();
    let mut shown = 0;
    for z in app.cf.zones.iter().filter(|z| q.is_empty() || z.name.contains(&q)) {
        shown += 1;
        let id = z.id.clone();
        let recs = app.cf.records.iter().find(|(zid, _)| *zid == z.id).map(|(_, r)| r.as_slice()).unwrap_or(&[]);
        let proxied = recs.iter().filter(|r| r.proxied).count();
        grid = grid.child(
            v_flex()
                .id(SharedString::from(format!("zone-{}", z.id)))
                .p_4()
                .gap_3()
                .rounded_lg()
                .border_1()
                .border_color(g.line)
                .bg(g.panel)
                .cursor_pointer()
                .hover(|d| d.bg(g.panel_hover).border_color(g.accent.opacity(0.5)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.cf.selected = Some(id.clone());
                    this.cf.filter_type = None;
                    cx.notify();
                }))
                .child(
                    h_flex()
                        .gap_3()
                        .child(avatar(&z.name, cx))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .child(div().font_weight(FontWeight::SEMIBOLD).child(z.name.clone()))
                                .child(div().text_xs().text_color(g.faint).child(trp("cf.records_count", &[("n", &recs.len().to_string())]))),
                        )
                        .child(pill(z.status.clone(), if z.status == "active" { g.ok } else { g.warn })),
                )
                .child(
                    h_flex()
                        .gap_3()
                        .text_xs()
                        .text_color(g.dim)
                        .child(h_flex().gap_1().children(brand("cloudflare", px(12.))).child(format!("{proxied} proxied")))
                        .when(!z.name_servers.is_empty(), |d| d.child(div().overflow_hidden().child(z.name_servers.join(", ")))),
                ),
        );
    }
    let add_zone = card(&tr("cf.add_zone"), cx).child(
        h_flex()
            .gap_2()
            .child(div().w(px(300.)).child(Input::new(&app.int.new_zone)))
            .child(
                Button::new("cf-add-zone")
                    .primary()
                    .icon(IconName::Plus)
                    .label(tr("cf.add_zone"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        let name = this.int.new_zone.read(cx).value().trim().to_lowercase();
                        let Some(account) = this.cf.accounts.first().cloned() else { return };
                        if !sesame_cloudflare::valid_hostname(&name) {
                            return;
                        }
                        this.request_api(
                            format!("add {name} to Cloudflare"),
                            vec![format!("account {}", account.name), "then change the domain's nameservers at the registrar".into()],
                            Risk::Change,
                            None,
                            ApiAction::CreateZone { account_id: account.id, name },
                            window,
                            cx,
                        );
                    })),
            )
            .child(div().text_xs().text_color(g.faint).child(tr("cf.add_zone_hint"))),
    );
    v_flex()
        .gap_4()
        .child(div().w(px(380.)).child(Input::new(&app.int.zone_search).prefix(icon(IconName::Search, g.faint)).cleanable(true)))
        .child(if shown == 0 { empty(tr("cf.no_domains"), cx) } else { grid })
        .child(add_zone)
}

fn zone_detail(app: &SesameApp, zone: &Zone, cx: &mut Context<SesameApp>) -> Div {
    let g = Glass::get(cx).clone();
    let zid = zone.id.clone();
    let records = app.cf.records.iter().find(|(id, _)| *id == zone.id).map(|(_, r)| r.clone()).unwrap_or_default();
    let q = app.int.record_search.read(cx).value().trim().to_lowercase();

    // Filters: type chips present in the zone + search.
    let mut types: Vec<String> = records.iter().map(|r| r.kind.clone()).collect();
    types.sort();
    types.dedup();
    let chip = |id: String, label: String, active: bool| {
        div()
            .id(SharedString::from(id))
            .px_3()
            .py_1()
            .rounded_full()
            .cursor_pointer()
            .text_xs()
            .border_1()
            .border_color(if active { g.accent } else { g.line })
            .when(active, |d| d.bg(g.accent_soft).text_color(g.text))
            .when(!active, |d| d.text_color(g.dim))
            .child(label)
    };
    let mut chips = h_flex().gap_1().child(
        chip("rt-all".into(), format!("{} · {}", tr("sites.all"), records.len()), app.cf.filter_type.is_none()).on_click(cx.listener(|this, _, _, cx| {
            this.cf.filter_type = None;
            cx.notify();
        })),
    );
    for t in &types {
        let n = records.iter().filter(|r| &r.kind == t).count();
        let tt = t.clone();
        chips = chips.child(
            chip(format!("rt-{t}"), format!("{t} · {n}"), app.cf.filter_type.as_deref() == Some(t.as_str())).on_click(cx.listener(move |this, _, _, cx| {
                this.cf.filter_type = Some(tt.clone());
                cx.notify();
            })),
        );
    }

    // Add form: type selector, name, content, priority (MX), TTL, proxy.
    let add_type = app.cf.add_type.clone();
    let mut type_sel = h_flex().p_0p5().gap_0p5().rounded_lg().bg(g.panel).border_1().border_color(g.line);
    for t in sesame_cloudflare::RECORD_TYPES {
        let active = add_type == *t;
        let tt = t.to_string();
        type_sel = type_sel.child(
            div()
                .id(SharedString::from(format!("add-{t}")))
                .px_2p5()
                .py_1()
                .rounded_md()
                .cursor_pointer()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .when(active, |d| d.bg(g.accent).text_color(g.accent_text))
                .when(!active, |d| d.text_color(g.dim).hover(|d| d.bg(g.panel_hover)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.cf.add_type = tt.clone();
                    if !sesame_cloudflare::proxiable(&tt) {
                        this.cf.add_proxied = false;
                    }
                    cx.notify();
                }))
                .child(t.to_string()),
        );
    }
    let ttl_opts: [(u32, String); 4] = [(1, tr("cf.auto")), (300, "5 min".into()), (3600, "1 h".into()), (86400, "1 d".into())];
    let mut ttl_sel = h_flex().gap_0p5();
    for (v, label) in ttl_opts {
        let active = app.cf.add_ttl == v;
        ttl_sel = ttl_sel.child(
            div()
                .id(SharedString::from(format!("ttl-{v}")))
                .px_2()
                .py_1()
                .rounded_md()
                .cursor_pointer()
                .text_xs()
                .when(active, |d| d.bg(g.accent_soft).text_color(g.text))
                .when(!active, |d| d.text_color(g.dim))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.cf.add_ttl = v;
                    cx.notify();
                }))
                .child(label),
        );
    }
    let can_proxy = sesame_cloudflare::proxiable(&add_type);
    let zname = zone.name.clone();
    let zid_add = zid.clone();
    let add_form = card(&tr("cf.add_record"), cx)
        .child(h_flex().gap_3().child(type_sel).child(div().text_xs().text_color(g.faint).child(tr("cf.add_hint"))))
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(v_flex().w(px(200.)).gap_1().child(div().text_xs().text_color(g.dim).child(tr("cf.name"))).child(Input::new(&app.int.rec_name)))
                .child(v_flex().flex_1().gap_1().child(div().text_xs().text_color(g.dim).child(tr("cf.content"))).child(Input::new(&app.int.rec_content)))
                .when(add_type == "MX", |d| {
                    d.child(v_flex().w(px(90.)).gap_1().child(div().text_xs().text_color(g.dim).child(tr("cf.priority"))).child(Input::new(&app.int.rec_priority)))
                }),
        )
        .child(
            h_flex()
                .gap_4()
                .child(h_flex().gap_2().child(div().text_xs().text_color(g.dim).child(tr("cf.ttl"))).child(ttl_sel))
                .when(can_proxy, |d| {
                    d.child(
                        gpui_kit::component::switch::Switch::new("add-proxied")
                            .checked(app.cf.add_proxied)
                            .label(tr("cf.proxied"))
                            .on_click(cx.listener(|this, v: &bool, _, cx| {
                                this.cf.add_proxied = *v;
                                cx.notify();
                            })),
                    )
                })
                .child(div().flex_1())
                .child(
                    Button::new("cf-add-record")
                        .primary()
                        .icon(IconName::Plus)
                        .label(tr("cf.add_record"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let name = this.int.rec_name.read(cx).value().trim().to_lowercase();
                            let content = this.int.rec_content.read(cx).value().trim().to_string();
                            let kind = this.cf.add_type.clone();
                            let fqdn = if name == "@" || name.is_empty() {
                                zname.clone()
                            } else if name == zname || name.ends_with(&format!(".{zname}")) {
                                name
                            } else {
                                format!("{name}.{zname}")
                            };
                            if !sesame_cloudflare::valid_hostname(&fqdn) || content.is_empty() {
                                return;
                            }
                            let priority = (kind == "MX").then(|| this.int.rec_priority.read(cx).value().trim().parse().unwrap_or(10));
                            let proxied = sesame_cloudflare::proxiable(&kind) && this.cf.add_proxied;
                            let rec = DnsRecord { id: String::new(), kind: kind.clone(), name: fqdn.clone(), content: content.clone(), proxied, ttl: this.cf.add_ttl, priority };
                            this.request_api(
                                format!("add {kind} {fqdn}"),
                                vec![format!("{kind} {fqdn} → {content}{}", if proxied { " (proxied)" } else { "" })],
                                Risk::Change,
                                None,
                                ApiAction::Upsert { zone_id: zid_add.clone(), records: vec![rec] },
                                window,
                                cx,
                            );
                        })),
                ),
        );

    // The table.
    let visible: Vec<DnsRecord> = records
        .into_iter()
        .filter(|r| app.cf.filter_type.as_ref().is_none_or(|t| &r.kind == t))
        .filter(|r| q.is_empty() || r.name.to_lowercase().contains(&q) || r.content.to_lowercase().contains(&q))
        .collect();
    let head = h_flex()
        .gap_3()
        .px_2()
        .pb_1()
        .text_xs()
        .text_color(g.faint)
        .child(div().w(px(64.)).child(tr("cf.type").to_uppercase()))
        .child(div().w(px(240.)).child(tr("cf.name").to_uppercase()))
        .child(div().flex_1().child(tr("cf.content").to_uppercase()))
        .child(div().w(px(130.)).child("PROXY"))
        .child(div().w(px(60.)).child(tr("cf.ttl").to_uppercase()))
        .child(div().w(px(90.)));
    let mut table = v_flex().gap_0p5().child(head);
    if visible.is_empty() {
        table = table.child(empty(tr("cf.no_records"), cx));
    }
    for r in visible {
        let (zid_d, rid, name) = (zid.clone(), r.id.clone(), r.name.clone());
        let zid_p = zid.clone();
        let mut toggled = r.clone();
        toggled.proxied = !toggled.proxied;
        let ttl = if r.ttl == 1 { tr("cf.auto") } else if r.ttl % 3600 == 0 { format!("{} h", r.ttl / 3600) } else { format!("{} s", r.ttl) };
        table = table.child(
            h_flex()
                .gap_3()
                .min_h(px(34.))
                .px_2()
                .rounded_md()
                .border_b_1()
                .border_color(g.line)
                .hover(|d| d.bg(g.panel_hover))
                .child(div().w(px(64.)).child(pill(r.kind.clone(), g.info)))
                .child(div().w(px(240.)).min_w_0().overflow_hidden().child(mono(r.name.clone())))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .child(mono(match r.priority {
                            Some(p) => format!("{p} {}", r.content),
                            None => r.content.clone(),
                        }).text_color(g.dim)),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("px-{}", r.id)))
                        .w(px(130.))
                        .when(sesame_cloudflare::proxiable(&r.kind), |d| {
                            d.cursor_pointer()
                                .child(pill(if r.proxied { tr("cf.proxied") } else { tr("cf.dns_only") }, if r.proxied { g.accent } else { g.faint }))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let title = format!("{} {}: {}", toggled.kind, toggled.name, if toggled.proxied { "proxied" } else { "DNS only" });
                                    this.request_api(title.clone(), vec![title], Risk::Change, None, ApiAction::Upsert { zone_id: zid_p.clone(), records: vec![toggled.clone()] }, window, cx);
                                }))
                        }),
                )
                .child(div().w(px(60.)).text_xs().text_color(g.dim).child(ttl))
                .child(
                    div().w(px(90.)).child(
                        Button::new(SharedString::from(format!("del-{}", r.id)))
                            .xsmall()
                            .danger()
                            .icon(IconName::X)
                            .label(tr("cf.delete"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.request_api(
                                    format!("delete DNS record {name}"),
                                    vec![format!("{name} stops resolving through this record")],
                                    Risk::Destructive,
                                    Some(name.clone()),
                                    ApiAction::Delete { zone_id: zid_d.clone(), record_id: rid.clone() },
                                    window,
                                    cx,
                                );
                            })),
                    ),
                ),
        );
    }
    let zid_purge = zid.clone();
    v_flex()
        .gap_4()
        .child(
            h_flex()
                .gap_3()
                .child(
                    h_flex()
                        .id("cf-back")
                        .gap_1()
                        .cursor_pointer()
                        .text_color(g.dim)
                        .hover(|d| d.text_color(g.text))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.cf.selected = None;
                            cx.notify();
                        }))
                        .child("←")
                        .child(tr("cf.all_domains")),
                )
                .child(div().flex_1())
                .child(pill(zone.status.clone(), if zone.status == "active" { g.ok } else { g.warn }))
                .child(
                    Button::new("cf-purge")
                        .ghost()
                        .icon(IconName::Zap)
                        .label(tr("cf.purge"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.request_api("purge Cloudflare cache".into(), vec!["every cached file of the zone".into()], Risk::Change, None, ApiAction::Purge { zone_id: zid_purge.clone() }, window, cx);
                        })),
                ),
        )
        .when(!zone.name_servers.is_empty(), |d| d.child(kv(&tr("cf.nameservers"), zone.name_servers.join(", "), cx)))
        .child(add_form)
        .child(
            card(&tr("cf.records"), cx)
                .child(h_flex().gap_3().child(chips).child(div().flex_1()).child(div().w(px(300.)).child(Input::new(&app.int.record_search).prefix(icon(IconName::Search, g.faint)).cleanable(true))))
                .child(table),
        )
}

/// Repository card on a site page.
pub fn repo_card(app: &SesameApp, server: ServerId, site: &Site, snap: &ServerSnapshot, cx: &mut Context<SesameApp>) -> Div {
    let g = Glass::get(cx).clone();
    let mut c = card(&tr("repo.title"), cx);
    let on_server = match &site.git {
        Some(git) => format!(
            "{} · {}{}",
            git.branch.clone().unwrap_or_else(|| "detached".into()),
            git.sha,
            git.github_repo.as_ref().map(|r| format!(" · {r}")).unwrap_or_default()
        ),
        None => tr("repo.not_git"),
    };
    c = c.child(kv(&tr("repo.on_server"), on_server, cx));
    let domain = site.domains.first().cloned().unwrap_or_default();
    let link = app.store.site_link(server, &domain).ok().flatten();
    if app.gh.login.is_none() {
        return c.child(div().text_xs().text_color(g.faint).child(trp("int.connect_first", &[("what", "GitHub")])));
    }
    match link {
        Some(link) => {
            let key = format!("{}@{}", link.repo, link.branch);
            let head = app.gh.heads.get(&key);
            let status = match (head, &site.git) {
                (Some(h), Some(gi)) if h.sha.starts_with(&gi.sha) => pill(tr("repo.up_to_date"), g.ok),
                (Some(_), _) => pill(tr("repo.behind"), g.warn),
                _ => div(),
            };
            let (r1, b1) = (link.repo.clone(), link.branch.clone());
            let link2 = link.clone();
            let snap2 = snap.clone();
            c = c
                .child(kv(&tr("repo.linked"), format!("{} · {}", link.repo, link.branch), cx))
                .when_some(head.cloned(), |d, h| {
                    d.child(kv(&tr("repo.latest"), format!("{} {} — {}", &h.sha[..7.min(h.sha.len())], h.message, h.author), cx))
                })
                .child(
                    h_flex()
                        .gap_2()
                        .child(status)
                        .child(
                            Button::new("repo-check")
                                .ghost()
                                .icon(IconName::RefreshCw)
                                .label(tr("repo.check"))
                                .on_click(cx.listener(move |this, _, _, cx| this.check_head(r1.clone(), b1.clone(), cx))),
                        )
                        .child(
                            Button::new("repo-deploy")
                                .primary()
                                .icon(IconName::Rocket)
                                .label(tr("repo.deploy"))
                                .on_click(cx.listener(move |this, _, window, cx| this.request_deploy(server, &link2, &snap2, window, cx))),
                        ),
                );
        }
        None => {
            let q = app.int.repo_search.read(cx).value().trim().to_lowercase();
            let suggested = site.git.as_ref().and_then(|g| g.github_repo.clone());
            let mut list = v_flex().gap_0p5();
            let matches: Vec<&Repo> = app
                .gh
                .repos
                .iter()
                .filter(|r| if q.is_empty() { suggested.as_deref() == Some(r.full_name.as_str()) } else { r.full_name.to_lowercase().contains(&q) })
                .take(6)
                .collect();
            for r in matches {
                let full = r.full_name.clone();
                let branches = app.gh.branches.get(&r.full_name).cloned();
                let mut row = h_flex()
                    .gap_2()
                    .min_h(px(30.))
                    .child(icon(if r.private { IconName::Lock } else { IconName::Globe }, g.faint).size_3())
                    .child(div().w(px(260.)).child(mono(r.full_name.clone())));
                match branches {
                    None => {
                        let f = full.clone();
                        row = row.child(
                            Button::new(SharedString::from(format!("br-{full}")))
                                .xsmall()
                                .ghost()
                                .label(tr("repo.branch"))
                                .on_click(cx.listener(move |this, _, _, cx| this.load_branches(f.clone(), cx))),
                        );
                    }
                    Some(bs) => {
                        let site_owned = site.clone();
                        let default = r.default_branch.clone();
                        let mut ordered = bs.clone();
                        ordered.sort_by_key(|b| (b != &default, b.clone()));
                        for b in ordered.into_iter().take(5) {
                            let (f, b2, s2) = (full.clone(), b.clone(), site_owned.clone());
                            row = row.child(
                                Button::new(SharedString::from(format!("link-{full}-{b}")))
                                    .xsmall()
                                    .primary()
                                    .icon(IconName::KeyRound)
                                    .label(b.clone())
                                    .tooltip(tr("repo.connect"))
                                    .on_click(cx.listener(move |this, _, window, cx| this.request_link(server, &s2, f.clone(), b2.clone(), window, cx))),
                            );
                        }
                    }
                }
                list = list.child(row);
            }
            c = c
                .child(div().w(px(380.)).child(Input::new(&app.int.repo_search).prefix(icon(IconName::Search, g.faint))))
                .child(list)
                .when(site.app_path.is_none(), |d| d.child(div().text_xs().text_color(g.faint).child(tr("repo.not_git"))));
        }
    }
    c.when_some(app.gh.error.clone(), |d, e| d.child(div().text_xs().text_color(g.bad).child(e)))
}

/// DNS card on a site page.
pub fn dns_card(app: &SesameApp, server: ServerId, site: &Site, cx: &mut Context<SesameApp>) -> Div {
    let g = Glass::get(cx).clone();
    let mut c = card(&tr("dns.title"), cx);
    if !app.cf.connected {
        return c.child(div().text_xs().text_color(g.faint).child(trp("int.connect_first", &[("what", "Cloudflare")])));
    }
    let Some(ip) = app.servers.iter().find(|s| s.id == server).and_then(|s| server_ip(&s.host)) else { return c };
    let mut fixable = Vec::new();
    for d in site.domains.iter().filter(|d| *d != "*") {
        let (text, color) = match app.dns_status(d, &ip) {
            Some(DnsStatus::Ok { record, .. }) => {
                (format!("{} {}", tr("dns.ok"), if record.proxied { "· proxied" } else { "· DNS only" }), g.ok)
            }
            Some(DnsStatus::Missing { zone }) => {
                fixable.push(d.clone());
                (trp("dns.missing", &[("zone", &zone)]), g.warn)
            }
            Some(DnsStatus::Elsewhere { record, .. }) => {
                fixable.push(d.clone());
                (trp("dns.elsewhere", &[("target", &record.content)]), g.warn)
            }
            _ => (tr("dns.no_zone"), g.faint),
        };
        c = c.child(h_flex().gap_3().min_h(px(26.)).child(dot(color)).child(div().w(px(240.)).child(mono(d.clone()))).child(div().text_color(g.dim).child(text)));
    }
    if !fixable.is_empty() {
        let ip2 = ip.clone();
        c = c.child(h_flex().child(
            Button::new("dns-point")
                .primary()
                .icon(IconName::Cloud)
                .label(tr("dns.point_here"))
                .on_click(cx.listener(move |this, _, window, cx| this.request_point_here(fixable.clone(), ip2.clone(), window, cx))),
        ));
    }
    c
}

/// Banner on the sites list: domains that are not pointed at the server yet.
pub fn dns_banner(app: &SesameApp, server: ServerId, snap: &ServerSnapshot, cx: &mut Context<SesameApp>) -> Option<Div> {
    if !app.cf.connected {
        return None;
    }
    let g = Glass::get(cx).clone();
    let ip = app.servers.iter().find(|s| s.id == server).and_then(|s| server_ip(&s.host))?;
    let hosts: Vec<String> = snap
        .sites
        .iter()
        .flat_map(|s| s.domains.iter())
        .filter(|d| *d != "*")
        .filter(|d| matches!(app.dns_status(d, &ip), Some(DnsStatus::Missing { .. })))
        .cloned()
        .collect();
    if hosts.is_empty() {
        return None;
    }
    let n = hosts.len().to_string();
    Some(
        h_flex()
            .gap_3()
            .p_3()
            .rounded_lg()
            .bg(g.accent_soft)
            .children(brand("cloudflare", px(16.)))
            .child(div().flex_1().child(hosts.join(", ")))
            .child(
                Button::new("dns-sync")
                    .primary()
                    .small()
                    .label(trp("dns.sync", &[("n", &n)]))
                    .on_click(cx.listener(move |this, _, window, cx| this.request_point_here(hosts.clone(), ip.clone(), window, cx))),
            ),
    )
}

/// Sidebar entry for the Cloudflare page.
pub fn sidebar_entry(app: &SesameApp, cx: &mut Context<SesameApp>) -> impl IntoElement {
    let g = Glass::get(cx).clone();
    let active = app.page == Page::Cloudflare;
    v_flex()
        .gap_0p5()
        .pt_3()
        .child(div().px_2().child(heading(&tr("nav.integrations"), cx)))
        .child(
            h_flex()
                .id("nav-cloudflare")
                .gap_2()
                .px_2()
                .h(px(30.))
                .rounded_md()
                .cursor_pointer()
                .when(active, |d| d.bg(g.panel_hover))
                .hover(|d| d.bg(g.panel_hover))
                .on_click(cx.listener(|this, _, _, cx| this.navigate(Page::Cloudflare, cx)))
                .children(brand("cloudflare", px(16.)))
                .child(div().flex_1().child(tr("nav.cloudflare")))
                .child(dot(if app.cf.connected { g.ok } else { g.faint })),
        )
}

impl SesameApp {
    /// Opens a zone on the Cloudflare page (screenshots, demos).
    #[doc(hidden)]
    pub fn demo_open_zone(&mut self, id: &str, cx: &mut Context<Self>) {
        self.cf.selected = Some(id.to_string());
        cx.notify();
    }

    /// Fills GitHub and Cloudflare with sample data (screenshots, demos).
    #[doc(hidden)]
    pub fn demo_integrations(&mut self, server_ip: &str, cx: &mut Context<Self>) {
        self.gh.login = Some("rasul-chopurov".into());
        self.gh.repos = vec![Repo {
            full_name: "rasul-chopurov/shop".into(),
            private: true,
            default_branch: "main".into(),
            ssh_url: "git@github.com:rasul-chopurov/shop.git".into(),
            pushed_at: None,
            description: None,
        }];
        self.gh.branches.insert("rasul-chopurov/shop".into(), vec!["main".into(), "staging".into()]);
        let zone = Zone { id: "z1".into(), name: "example.com".into(), status: "active".into(), name_servers: vec!["ada.ns.cloudflare.com".into(), "bob.ns.cloudflare.com".into()], paused: false };
        let rec = |kind: &str, name: &str, content: &str, proxied: bool| DnsRecord { id: format!("{kind}-{name}"), kind: kind.into(), name: name.into(), content: content.into(), proxied, ttl: 1, priority: None };
        self.cf.connected = true;
        self.cf.accounts = vec![Account { id: "a1".into(), name: "Shop LLC".into() }];
        self.cf.add_type = "A".into();
        self.cf.add_ttl = 1;
        self.cf.add_proxied = true;
        self.cf.zones = vec![zone, Zone { id: "z2".into(), name: "response.az".into(), status: "active".into(), name_servers: vec![], paused: false }];
        self.cf.records = vec![
            ("z1".into(), vec![rec("A", "example.com", server_ip, true), rec("MX", "example.com", "mx.zoho.eu", false), rec("TXT", "example.com", "v=spf1 include:zoho.eu ~all", false)]),
            ("z2".into(), vec![rec("A", "sesame.response.az", server_ip, true)]),
        ];
        self.cf.selected = None;
        cx.notify();
    }
}
