//! The desktop window: servers on the left, the selected server's discovered
//! picture on the right.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::tag::Tag;
use gpui_kit::component::{ActiveTheme, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use sesame_core::secrets::{self, SecretKind};
use sesame_core::snapshot::*;
use sesame_core::{AuthMethod, ServerConfig, ServerId, SqliteStore, Store};
use sesame_discovery::insights::{self, Level};
use sesame_ssh::HostKeyStatus;

use crate::{format, runtime};

pub fn run() {
    gpui_kit::application().run(|cx| {
        gpui_kit::init(cx);
        Theme::sync_system_appearance(None, cx);
        let store = match SqliteStore::open_default() {
            Ok(s) => Arc::new(s),
            Err(e) => {
                eprintln!("cannot open Sesame state: {e:#}");
                cx.quit();
                return;
            }
        };
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions { title: Some("Sesame".into()), ..Default::default() }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1280.), px(820.)), cx))),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| SesameApp::new(store, window, cx)))
            .expect("failed to open window");
        cx.activate(true);
    });
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Overview,
    Sites,
    Services,
    Databases,
    MailCron,
    Security,
}

impl Tab {
    pub const ALL: [Tab; 6] = [Tab::Overview, Tab::Sites, Tab::Services, Tab::Databases, Tab::MailCron, Tab::Security];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Sites => "Sites",
            Tab::Services => "Processes",
            Tab::Databases => "Databases",
            Tab::MailCron => "Mail & Cron",
            Tab::Security => "Security",
        }
    }
}

struct AddForm {
    name: Entity<InputState>,
    target: Entity<InputState>,
    key: Entity<InputState>,
    password: Entity<InputState>,
    error: Option<String>,
}

pub struct SesameApp {
    store: Arc<SqliteStore>,
    servers: Vec<ServerConfig>,
    selected: Option<ServerId>,
    adding: bool,
    tab: Tab,
    snapshots: HashMap<ServerId, ServerSnapshot>,
    busy: HashSet<ServerId>,
    errors: HashMap<ServerId, String>,
    notices: HashMap<ServerId, String>,
    form: AddForm,
}

impl SesameApp {
    pub fn new(store: Arc<SqliteStore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let servers = store.servers().unwrap_or_default();
        let snapshots = servers
            .iter()
            .filter_map(|s| Some((s.id, store.snapshot(s.id).ok()??)))
            .collect();
        let input = |placeholder: &str, window: &mut Window, cx: &mut Context<Self>| {
            let placeholder = placeholder.to_string();
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        let form = AddForm {
            name: input("production", window, cx),
            target: input("root@203.0.113.10  or  deploy@example.com:2222", window, cx),
            key: input("~/.ssh/id_ed25519  (empty = ssh-agent and default keys)", window, cx),
            password: cx.new(|cx| {
                InputState::new(window, cx).placeholder("only if the server uses password login").masked(true)
            }),
            error: None,
        };
        Self {
            selected: servers.first().map(|s| s.id),
            adding: servers.is_empty(),
            servers,
            store,
            tab: Tab::Overview,
            snapshots,
            busy: HashSet::new(),
            errors: HashMap::new(),
            notices: HashMap::new(),
            form,
        }
    }

    pub fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.adding = false;
        cx.notify();
    }

    pub fn show_add_form(&mut self, cx: &mut Context<Self>) {
        self.adding = true;
        cx.notify();
    }

    fn submit_add(&mut self, cx: &mut Context<Self>) {
        let name = self.form.name.read(cx).value().trim().to_string();
        let target = self.form.target.read(cx).value().trim().to_string();
        let key = self.form.key.read(cx).value().trim().to_string();
        let password = self.form.password.read(cx).value().to_string();
        let name = if name.is_empty() { target.split('@').next_back().unwrap_or("").to_string() } else { name };

        let mut server = match crate::cli::parse_target(&name, &target, (!key.is_empty()).then_some(key)) {
            Ok(s) => s,
            Err(e) => {
                self.form.error = Some(e.to_string());
                cx.notify();
                return;
            }
        };
        if !password.is_empty() {
            server.auth = AuthMethod::Password;
            if let Err(e) = secrets::set(server.id, SecretKind::Password, &password) {
                self.form.error = Some(format!("Keychain: {e}"));
                cx.notify();
                return;
            }
        }
        if let Err(e) = self.store.save_server(&server) {
            self.form.error = Some(e.to_string());
            cx.notify();
            return;
        }
        self.form.error = None;
        let id = server.id;
        self.servers.push(server);
        self.selected = Some(id);
        self.adding = false;
        self.discover(id, cx);
    }

    fn discover(&mut self, id: ServerId, cx: &mut Context<Self>) {
        let Some(server) = self.servers.iter().find(|s| s.id == id).cloned() else { return };
        if !self.busy.insert(id) {
            return;
        }
        self.errors.remove(&id);
        cx.notify();
        let job = runtime::get().spawn(async move { sesame_discovery::discover_server(&server).await });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |this, cx| {
                this.busy.remove(&id);
                match result {
                    Ok(Ok((snap, host_key))) => {
                        let _ = this.store.save_snapshot(id, &snap);
                        this.snapshots.insert(id, snap);
                        if let HostKeyStatus::Learned { fingerprint } = host_key {
                            this.notices.insert(id, format!("New server key trusted and saved to known_hosts: {fingerprint}"));
                        }
                    }
                    Ok(Err(e)) => {
                        this.errors.insert(id, format!("{e:#}"));
                    }
                    Err(e) => {
                        this.errors.insert(id, format!("discovery crashed: {e}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn remove_selected(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected else { return };
        let _ = self.store.delete_server(id);
        let _ = secrets::delete(id, SecretKind::Password);
        self.servers.retain(|s| s.id != id);
        self.snapshots.remove(&id);
        self.selected = self.servers.first().map(|s| s.id);
        self.adding = self.servers.is_empty();
        cx.notify();
    }
}

impl Render for SesameApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        let (bg, fg) = (t.background, t.foreground);
        h_flex()
            .size_full()
            .bg(bg)
            .text_color(fg)
            .text_sm()
            .child(self.render_sidebar(cx))
            .child(
                v_flex().flex_1().h_full().overflow_hidden().child(if self.adding {
                    self.render_add_form(cx).into_any_element()
                } else if let Some(server) = self.selected.and_then(|id| self.servers.iter().find(|s| s.id == id)).cloned() {
                    self.render_server(&server, cx).into_any_element()
                } else {
                    empty_state("Add a server to begin", cx).into_any_element()
                }),
            )
    }
}

impl SesameApp {
    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        let (sidebar, border, muted, accent, danger, success) =
            (t.sidebar, t.border, t.muted_foreground, t.accent, t.danger, t.success);
        let mut list = v_flex().gap_0p5();
        for s in &self.servers {
            let id = s.id;
            let active = !self.adding && self.selected == Some(id);
            let dot = if self.busy.contains(&id) {
                muted
            } else if self.errors.contains_key(&id) {
                danger
            } else if self.snapshots.contains_key(&id) {
                success
            } else {
                muted
            };
            list = list.child(
                h_flex()
                    .id(SharedString::from(format!("server-{id}")))
                    .gap_2()
                    .px_2()
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .when(active, |d| d.bg(accent))
                    .hover(|d| d.bg(accent))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = Some(id);
                        this.adding = false;
                        cx.notify();
                    }))
                    .child(div().size(px(8.)).rounded_full().bg(dot))
                    .child(
                        v_flex()
                            .overflow_hidden()
                            .child(div().font_weight(FontWeight::MEDIUM).child(s.name.clone()))
                            .child(div().text_xs().text_color(muted).child(s.host.clone())),
                    ),
            );
        }
        v_flex()
            .w(px(250.))
            .h_full()
            .flex_shrink_0()
            .bg(sidebar)
            .border_r_1()
            .border_color(border)
            .p_3()
            .gap_3()
            .child(
                v_flex()
                    .pt_6()
                    .child(div().text_lg().font_weight(FontWeight::BOLD).child("Sesame"))
                    .child(div().text_xs().text_color(muted).child("Open your servers")),
            )
            .child(
                Button::new("add-server")
                    .primary()
                    .label("Add server")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.adding = true;
                        cx.notify();
                    })),
            )
            .child(div().text_xs().text_color(muted).child("SERVERS"))
            .child(list)
    }

    fn render_add_form(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        let (muted, danger) = (t.muted_foreground, t.danger);
        let field = |label: &str, hint: &str, input: &Entity<InputState>| {
            v_flex()
                .gap_1()
                .child(div().font_weight(FontWeight::MEDIUM).child(label.to_string()))
                .child(Input::new(input))
                .child(div().text_xs().text_color(muted).child(hint.to_string()))
        };
        v_flex()
            .p_8()
            .gap_5()
            .max_w(px(620.))
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_xl().font_weight(FontWeight::BOLD).child("Add a server"))
                    .child(div().text_color(muted).child(
                        "Give Sesame SSH access. It connects, looks around read-only and shows you every site, database, mail setup and process it finds.",
                    )),
            )
            .child(field("SSH address", "user@host[:port] — root or a user with passwordless sudo sees everything", &self.form.target))
            .child(field("Name", "Shown in the sidebar", &self.form.name))
            .child(field("Private key", "Optional", &self.form.key))
            .child(field("Password", "Optional — stored in the macOS Keychain, never on disk", &self.form.password))
            .when_some(self.form.error.clone(), |d, e| d.child(div().text_color(danger).child(e)))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("connect")
                            .primary()
                            .label("Open Sesame — connect & discover")
                            .on_click(cx.listener(|this, _, _, cx| this.submit_add(cx))),
                    )
                    .when(!self.servers.is_empty(), |d| {
                        d.child(Button::new("cancel").ghost().label("Cancel").on_click(cx.listener(|this, _, _, cx| {
                            this.adding = false;
                            cx.notify();
                        })))
                    }),
            )
    }

    fn render_server(&self, server: &ServerConfig, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.theme();
        let (border, muted, danger) = (t.border, t.muted_foreground, t.danger);
        let id = server.id;
        let busy = self.busy.contains(&id);
        let snap = self.snapshots.get(&id);

        let header = h_flex()
            .px_6()
            .pt_8()
            .pb_3()
            .gap_3()
            .items_end()
            .child(
                v_flex()
                    .flex_1()
                    .child(div().text_xl().font_weight(FontWeight::BOLD).child(server.name.clone()))
                    .child(div().text_color(muted).child(format!(
                        "{}{}",
                        server.address(),
                        snap.map(|s| format!(" · discovered {}", s.taken_at.replace('T', " ").trim_end_matches('Z')))
                            .unwrap_or_default()
                    ))),
            )
            .child(
                Button::new("discover")
                    .primary()
                    .loading(busy)
                    .label(if busy { "Discovering…" } else if snap.is_some() { "Rediscover" } else { "Discover" })
                    .on_click(cx.listener(move |this, _, _, cx| this.discover(id, cx))),
            )
            .child(Button::new("remove").ghost().label("Remove").on_click(cx.listener(|this, _, _, cx| this.remove_selected(cx))));

        let mut tabs = h_flex().px_6().gap_1().pb_2().border_b_1().border_color(border);
        for tab in Tab::ALL {
            let b = Button::new(SharedString::from(format!("tab-{}", tab.label())))
                .label(tab.label())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tab = tab;
                    cx.notify();
                }));
            tabs = tabs.child(if self.tab == tab { b.primary() } else { b.ghost() });
        }

        let mut body = v_flex().gap_4();
        if let Some(e) = self.errors.get(&id) {
            body = body.child(card("Could not discover", cx).child(div().text_color(danger).child(e.clone())));
        }
        if let Some(n) = self.notices.get(&id) {
            body = body.child(div().text_xs().text_color(muted).child(n.clone()));
        }
        body = match snap {
            None if busy => body.child(empty_state("Connecting and looking around…", cx)),
            None => body.child(empty_state("Not discovered yet — press Discover", cx)),
            Some(s) => match self.tab {
                Tab::Overview => body.child(overview(s, cx)),
                Tab::Sites => body.child(sites(s, cx)),
                Tab::Services => body.child(processes(s, cx)),
                Tab::Databases => body.child(databases(s, cx)),
                Tab::MailCron => body.child(mail_cron(s, cx)),
                Tab::Security => body.child(security(s, cx)),
            },
        };

        v_flex()
            .size_full()
            .child(header)
            .child(tabs)
            .child(div().id("content").flex_1().overflow_y_scroll().p_6().child(body))
    }
}

// ---- building blocks ------------------------------------------------------

fn empty_state(text: &str, cx: &App) -> Div {
    div()
        .p_10()
        .flex()
        .justify_center()
        .text_color(cx.theme().muted_foreground)
        .child(text.to_string())
}

fn card(title: &str, cx: &App) -> Div {
    let t = cx.theme();
    v_flex()
        .gap_2()
        .p_4()
        .rounded_lg()
        .border_1()
        .border_color(t.border)
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.muted_foreground)
                .child(title.to_uppercase()),
        )
}

fn kv(label: &str, value: impl Into<SharedString>, cx: &App) -> Div {
    h_flex()
        .gap_4()
        .child(div().w(px(150.)).flex_shrink_0().text_color(cx.theme().muted_foreground).child(label.to_string()))
        .child(div().flex_1().min_w_0().overflow_hidden().child(value.into()))
}

fn state_tag(state: &str) -> Tag {
    let tag = match state {
        "RUNNING" | "running" | "active" => Tag::success(),
        "STARTING" | "BACKOFF" | "inactive" => Tag::warning(),
        _ => Tag::danger(),
    };
    tag.child(state.to_string())
}

fn grid() -> Div {
    div().grid().grid_cols(2).gap_4()
}

// ---- tabs -----------------------------------------------------------------

fn overview(s: &ServerSnapshot, cx: &App) -> Div {
    let os = &s.os;
    let mem_used = os.mem_total_bytes.saturating_sub(os.mem_available_bytes);
    let list = insights::insights(s, time::OffsetDateTime::now_utc());

    let mut attention = card("Needs attention", cx);
    if list.is_empty() {
        attention = attention.child(div().child("Nothing — all good."));
    }
    for i in &list {
        let tag = match i.level {
            Level::Critical => Tag::danger().child("critical"),
            Level::Warning => Tag::warning().child("warning"),
            Level::Info => Tag::info().child("info"),
        };
        attention = attention.child(h_flex().gap_2().child(tag).child(div().flex_1().child(i.message.clone())));
    }
    for w in &s.warnings {
        attention = attention.child(h_flex().gap_2().child(Tag::secondary().child("partial")).child(w.clone()));
    }

    let web = s
        .web_servers
        .iter()
        .map(|w| w.version.clone().unwrap_or_else(|| format!("{:?}", w.kind)))
        .collect::<Vec<_>>()
        .join(", ");
    let php = s.php.iter().map(|p| p.version.clone()).collect::<Vec<_>>().join(", ");
    let dbs = s
        .databases
        .iter()
        .map(|d| format!("{:?} {}", d.kind, d.version.clone().unwrap_or_default()))
        .collect::<Vec<_>>()
        .join(", ");

    v_flex()
        .gap_4()
        .child(attention)
        .child(
            grid()
                .child(
                    card("Machine", cx)
                        .child(kv("Hostname", os.hostname.clone(), cx))
                        .child(kv("System", os.distro.clone(), cx))
                        .child(kv("Kernel", format!("{} ({})", os.kernel, os.arch), cx))
                        .child(kv("CPUs", os.cpus.to_string(), cx))
                        .child(kv("Uptime", format::duration(os.uptime_seconds), cx))
                        .child(kv("Access", if os.privileged { "root / sudo" } else { "limited user" }, cx)),
                )
                .child(
                    card("Resources", cx)
                        .child(kv(
                            "Memory",
                            format!(
                                "{} of {} ({}%)",
                                format::bytes(mem_used),
                                format::bytes(os.mem_total_bytes),
                                format::percent(mem_used, os.mem_total_bytes)
                            ),
                            cx,
                        ))
                        .child(kv(
                            "Disk /",
                            format!(
                                "{} of {} ({}%)",
                                format::bytes(os.disk_used_bytes),
                                format::bytes(os.disk_total_bytes),
                                format::percent(os.disk_used_bytes, os.disk_total_bytes)
                            ),
                            cx,
                        ))
                        .child(kv("Load", format!("{:.2}  {:.2}  {:.2}", os.load[0], os.load[1], os.load[2]), cx)),
                )
                .child(
                    card("Stack", cx)
                        .child(kv("Web", if web.is_empty() { "—".into() } else { web }, cx))
                        .child(kv("PHP", if php.is_empty() { "—".into() } else { php }, cx))
                        .child(kv("Data", if dbs.is_empty() { "—".into() } else { dbs }, cx)),
                )
                .child(
                    card("Found", cx)
                        .child(kv("Sites", s.sites.len().to_string(), cx))
                        .child(kv("Processes", s.supervisor.len().to_string(), cx))
                        .child(kv("Databases", s.databases.iter().map(|d| d.databases.len()).sum::<usize>().to_string(), cx))
                        .child(kv("Cron jobs", s.cron.len().to_string(), cx))
                        .child(kv("Certificates", s.certificates.len().to_string(), cx)),
                ),
        )
}

fn sites(s: &ServerSnapshot, cx: &App) -> Div {
    let mut col = v_flex().gap_4();
    if s.sites.is_empty() {
        return col.child(empty_state("No sites found in the web server config", cx));
    }
    for site in &s.sites {
        let title = if site.domains == ["*"] { "Any host (default)".to_string() } else { site.domains.join("  ·  ") };
        let mut c = card(&title, cx);
        let fw = match (site.framework, &site.framework_version) {
            (Framework::Unknown, _) => "—".to_string(),
            (f, Some(v)) => format!("{f:?} {v}"),
            (f, None) => format!("{f:?}"),
        };
        c = c
            .child(kv("Framework", fw, cx))
            .child(kv("Directory", site.app_path.clone().unwrap_or_else(|| "—".into()), cx))
            .child(kv(
                "Served by",
                format!(
                    "{}{}{}",
                    site.served_by.map(|k| format!("{k:?}")).unwrap_or_default(),
                    if site.php { " + PHP" } else { "" },
                    if site.upstreams.is_empty() { String::new() } else { format!(" → {}", site.upstreams.join(", ")) }
                ),
                cx,
            ))
            .child(kv("TLS", site.tls.clone().unwrap_or_else(|| "none".into()), cx));
        if let Some(env) = &site.env {
            let db = match (&env.db_connection, &env.db_database) {
                (Some(c), Some(d)) => format!("{c} · {d} @ {}", env.db_host.clone().unwrap_or_default()),
                (Some(c), None) => c.clone(),
                _ => "—".into(),
            };
            let drivers = format!(
                "cache {} · queue {} · session {}",
                env.cache_store.as_deref().unwrap_or("—"),
                env.queue_connection.as_deref().unwrap_or("—"),
                env.session_driver.as_deref().unwrap_or("—")
            );
            c = c
                .child(kv("Environment", env.app_env.clone().unwrap_or_else(|| "—".into()), cx))
                .child(kv("Database", db, cx))
                .child(kv("Drivers", drivers, cx))
                .child(kv(
                    "Mail",
                    match (&env.mail_mailer, &env.mail_host) {
                        (Some(m), Some(h)) => format!("{m} via {h}"),
                        (Some(m), None) => m.clone(),
                        _ => "—".into(),
                    },
                    cx,
                ));
        }
        if !site.processes.is_empty() {
            let mut row = h_flex().gap_1().flex_wrap();
            for name in &site.processes {
                let state = s.supervisor.iter().find(|p| &p.name == name).map(|p| p.state.as_str()).unwrap_or("?");
                row = row.child(state_tag(state).child(format!(" {name}")));
            }
            c = c.child(h_flex().gap_4().child(div().w(px(150.)).text_color(cx.theme().muted_foreground).child("Processes")).child(row));
        }
        col = col.child(c);
    }
    col
}

fn processes(s: &ServerSnapshot, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    let mut sup = card("Supervisor", cx);
    if s.supervisor.is_empty() {
        sup = sup.child(div().text_color(muted).child("No supervisor programs"));
    }
    for p in &s.supervisor {
        sup = sup.child(
            h_flex()
                .gap_3()
                .child(div().w(px(90.)).child(state_tag(&p.state)))
                .child(div().w(px(260.)).font_weight(FontWeight::MEDIUM).child(p.name.clone()))
                .child(div().w(px(80.)).text_color(muted).child(format!("{:?}", p.role).to_lowercase()))
                .child(div().w(px(80.)).text_color(muted).child(p.user.clone().unwrap_or_default()))
                .child(div().flex_1().text_color(muted).child(p.uptime.clone().map(|u| format!("up {u}")).unwrap_or_default())),
        );
    }
    let mut sys = card("System services", cx);
    for svc in s.services.iter().filter(|x| x.active != "inactive") {
        sys = sys.child(
            h_flex()
                .gap_3()
                .child(div().w(px(90.)).child(state_tag(&svc.sub)))
                .child(div().w(px(260.)).child(svc.unit.trim_end_matches(".service").to_string()))
                .child(div().flex_1().text_color(muted).child(svc.description.clone())),
        );
    }
    v_flex().gap_4().child(sup).child(sys)
}

fn databases(s: &ServerSnapshot, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    let mut col = v_flex().gap_4();
    if s.databases.is_empty() {
        return col.child(empty_state("No database servers found", cx));
    }
    for d in &s.databases {
        let mut c = card(&format!("{:?}", d.kind), cx)
            .child(kv("Version", d.version.clone().unwrap_or_else(|| "—".into()), cx))
            .child(kv("Listens on", d.listen.clone().unwrap_or_else(|| "—".into()), cx));
        let used_by: Vec<String> = s
            .sites
            .iter()
            .filter(|site| {
                site.env.as_ref().is_some_and(|e| {
                    e.db_database.as_ref().is_some_and(|db| d.databases.contains(db))
                        || (d.kind == DatabaseKind::Redis && e.redis_host.is_some())
                })
            })
            .map(|site| site.domains.first().cloned().unwrap_or_default())
            .collect();
        if d.databases.is_empty() {
            c = c.child(kv("Databases", "needs credentials to list", cx));
        } else {
            c = c.child(kv("Databases", d.databases.join(", "), cx));
        }
        if !used_by.is_empty() {
            c = c.child(kv("Used by", used_by.join(", "), cx));
        }
        col = col.child(c);
    }
    col.child(div().text_xs().text_color(muted).child("Browsing tables and running SQL comes in the next step."))
}

fn mail_cron(s: &ServerSnapshot, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    let mut mail = card("Mail", cx);
    if s.mail.servers.is_empty() {
        mail = mail.child(kv("Mail server", "none on this machine", cx));
        if !s.mail.external.is_empty() {
            mail = mail.child(kv("Apps send via", s.mail.external.join(", "), cx));
        }
    } else {
        mail = mail
            .child(kv("Running", s.mail.servers.join(", "), cx))
            .child(kv("Hostname", s.mail.hostname.clone().unwrap_or_else(|| "—".into()), cx))
            .child(kv("Domains", if s.mail.domains.is_empty() { "—".into() } else { s.mail.domains.join(", ") }, cx));
    }
    let mut cron = card("Cron", cx);
    if s.cron.is_empty() {
        cron = cron.child(div().text_color(muted).child("No cron jobs"));
    }
    for c in &s.cron {
        cron = cron.child(
            h_flex()
                .gap_3()
                .child(div().w(px(110.)).font_family("Menlo").child(c.schedule.clone()))
                .child(div().w(px(70.)).text_color(muted).child(c.user.clone()))
                .child(div().flex_1().font_family("Menlo").text_xs().child(c.command.clone())),
        );
    }
    v_flex().gap_4().child(mail).child(cron)
}

fn security(s: &ServerSnapshot, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    let fw = card("Firewall", cx)
        .child(kv("ufw", s.firewall.ufw.clone().unwrap_or_else(|| "not installed".into()), cx))
        .child(kv(
            "fail2ban",
            if s.firewall.fail2ban_jails.is_empty() { "no jails".into() } else { s.firewall.fail2ban_jails.join(", ") },
            cx,
        ));
    let mut ports = card("Listening ports", cx);
    for p in &s.ports {
        ports = ports.child(
            h_flex()
                .gap_3()
                .child(div().w(px(70.)).font_family("Menlo").child(p.port.to_string()))
                .child(div().w(px(90.)).child(if p.public { Tag::warning().child("public") } else { Tag::secondary().child("local") }))
                .child(div().w(px(140.)).text_color(muted).child(p.address.clone()))
                .child(div().flex_1().child(p.process.clone())),
        );
    }
    let mut certs = card("Certificates", cx);
    if s.certificates.is_empty() {
        certs = certs.child(div().text_color(muted).child("No certificate files found (Caddy may manage them automatically)"));
    }
    for c in &s.certificates {
        certs = certs.child(
            v_flex()
                .child(div().font_weight(FontWeight::MEDIUM).child(c.path.clone()))
                .child(div().text_xs().text_color(muted).child(format!(
                    "{} · expires {}",
                    c.names.join(", "),
                    c.not_after.clone().unwrap_or_else(|| "?".into())
                ))),
        );
    }
    v_flex().gap_4().child(fw).child(ports).child(certs)
}
