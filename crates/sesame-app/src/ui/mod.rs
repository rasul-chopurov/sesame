//! The desktop window: glass titlebar with breadcrumbs, a sidebar tree of
//! servers and their sections, the content panel and a status bar.

mod integrations;
mod files;
mod jobs;
mod newsite;
mod terminal;
pub mod theme;
mod views;
mod widgets;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{TitleBar, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use sesame_core::secrets::{self, SecretKind};
use sesame_core::snapshot::*;
use sesame_core::{AuthMethod, ServerConfig, ServerId, SqliteStore, Store};
use sesame_ssh::HostKeyStatus;
use sesame_update::{Channel, UpdateInfo};

use crate::i18n::{self, LangPref, tr, trp};
use crate::runtime;
use theme::{Glass, ThemePref};
use widgets::*;

pub fn run() {
    gpui_kit::application().with_assets(crate::assets::AppAssets).run(|cx| {
        gpui_kit::init(cx);
        let store = match SqliteStore::open_default() {
            Ok(s) => Arc::new(s),
            Err(e) => {
                eprintln!("cannot open Sesame state: {e:#}");
                cx.quit();
                return;
            }
        };
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1320.), px(840.)), cx))),
            window_min_size: Some(size(px(960.), px(600.))),
            window_background: WindowBackgroundAppearance::Blurred,
            ..TitleBar::window_options()
        };
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| SesameApp::new(store, true, window, cx)))
            .expect("failed to open window");
        cx.activate(true);
    });
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Overview,
    Sites,
    Site(usize),
    Processes,
    Services,
    Databases,
    MailCron,
    Security,
    NewSite,
    Files(usize),
    Terminal,
}

impl Section {
    pub const NAV: [Section; 8] = [
        Section::Overview,
        Section::Sites,
        Section::Processes,
        Section::Services,
        Section::Databases,
        Section::MailCron,
        Section::Security,
        Section::Terminal,
    ];

    pub fn label(self) -> String {
        tr(match self {
            Section::Overview => "nav.overview",
            Section::Sites | Section::Site(_) | Section::NewSite | Section::Files(_) => "nav.sites",
            Section::Processes => "nav.processes",
            Section::Services => "nav.services",
            Section::Databases => "nav.databases",
            Section::MailCron => "nav.mail_cron",
            Section::Security => "nav.security",
            Section::Terminal => "nav.terminal",
        })
    }

    pub fn slug(self) -> &'static str {
        match self {
            Section::Overview => "overview",
            Section::Sites | Section::Site(_) | Section::NewSite | Section::Files(_) => "sites",
            Section::Processes => "processes",
            Section::Services => "services",
            Section::Databases => "databases",
            Section::MailCron => "mail-cron",
            Section::Security => "security",
            Section::Terminal => "terminal",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Section::Overview => IconName::Gauge,
            Section::Sites | Section::Site(_) | Section::NewSite | Section::Files(_) => IconName::Globe,
            Section::Processes => IconName::Activity,
            Section::Services => IconName::Package,
            Section::Databases => IconName::Database,
            Section::MailCron => IconName::Mail,
            Section::Security => IconName::Shield,
            Section::Terminal => IconName::Terminal,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Server(ServerId, Section),
    AddServer,
    Settings,
    Updates,
    /// Create or unlock the encrypted vault; shown before anything else.
    Vault,
    Cloudflare,
}

#[derive(Default)]
pub struct UpdateState {
    pub checking: bool,
    pub checked: bool,
    pub info: Option<UpdateInfo>,
    pub error: Option<String>,
    pub downloading: bool,
    /// Installed and waiting for a restart.
    pub ready: Option<PathBuf>,
}

pub(crate) struct AddForm {
    pub name: Entity<InputState>,
    pub target: Entity<InputState>,
    pub key: Entity<InputState>,
    pub password: Entity<InputState>,
    pub error: Option<String>,
}

pub struct SesameApp {
    pub(crate) store: Arc<SqliteStore>,
    pub(crate) servers: Vec<ServerConfig>,
    pub(crate) page: Page,
    pub(crate) snapshots: HashMap<ServerId, ServerSnapshot>,
    pub(crate) busy: HashSet<ServerId>,
    pub(crate) errors: HashMap<ServerId, String>,
    pub(crate) notices: HashMap<ServerId, String>,
    pub(crate) form: AddForm,
    pub(crate) filter: Entity<InputState>,
    pub(crate) site_search: Entity<InputState>,
    pub(crate) site_kind: Option<Framework>,
    pub(crate) theme_pref: ThemePref,
    pub(crate) lang_pref: LangPref,
    pub(crate) translucent: bool,
    pub(crate) update: UpdateState,
    pub(crate) pending: Option<jobs::Pending>,
    pub(crate) job: Option<jobs::Job>,
    pub(crate) db_name: Entity<InputState>,
    pub(crate) db_user: Entity<InputState>,
    pub(crate) db_search: Entity<InputState>,
    pub(crate) db_engine: Option<DatabaseKind>,
    /// Some(true) = only databases a site uses, Some(false) = only unused.
    pub(crate) db_filter: Option<bool>,
    pub(crate) vault_pw: Entity<InputState>,
    pub(crate) vault_pw2: Entity<InputState>,
    pub(crate) vault_busy: bool,
    pub(crate) vault_error: Option<String>,
    pub(crate) gh: integrations::GhState,
    pub(crate) cf: integrations::CfState,
    pub(crate) int: integrations::Inputs,
    pub(crate) ns: newsite::Form,
    /// Open SSH tunnels to admin panels, by server and remote port.
    pub(crate) tunnels: HashMap<(ServerId, u16), sesame_ssh::Tunnel>,
    pub(crate) files: Option<files::FilesState>,
    pub(crate) files_in: files::Inputs,
    pub(crate) terminal: Option<terminal::TerminalState>,
    _subscriptions: Vec<Subscription>,
}

impl SesameApp {
    pub fn new(store: Arc<SqliteStore>, translucent: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let theme_pref = ThemePref::parse(&store.setting_or("theme", "system"));
        let lang_pref = LangPref::parse(&store.setting_or("language", "system"));
        i18n::apply(lang_pref);
        theme::apply(theme::is_dark(theme_pref, Some(window)), translucent, cx);

        let servers = store.servers().unwrap_or_default();
        let snapshots = servers.iter().filter_map(|s| Some((s.id, store.snapshot(s.id).ok()??))).collect();

        let input = |placeholder: String, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        let form = AddForm {
            name: input("production".into(), window, cx),
            target: input("root@203.0.113.10  ·  deploy@example.com:2222".into(), window, cx),
            key: input(tr("add.key_placeholder"), window, cx),
            password: cx.new(|cx| InputState::new(window, cx).placeholder(tr("add.password_placeholder")).masked(true)),
            error: None,
        };
        let filter = input(tr("nav.filter"), window, cx);
        let site_search = input(tr("sites.search"), window, cx);
        let db_name = input(tr("db.name_placeholder"), window, cx);
        let db_user = input(tr("db.user_placeholder"), window, cx);
        let db_search = input(tr("db.search"), window, cx);
        let int = integrations::Inputs::new(window, cx);
        let ns = newsite::Form::new(window, cx);
        let files_in = files::Inputs::new(window, cx);
        let vault_pw = cx.new(|cx| InputState::new(window, cx).masked(true));
        let vault_pw2 = cx.new(|cx| InputState::new(window, cx).masked(true));

        let mut subs = vec![
            cx.subscribe(&filter, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&site_search, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        subs.push(cx.subscribe(&db_name, |_, _, _: &InputEvent, cx| cx.notify()));
        subs.push(cx.subscribe(&db_user, |_, _, _: &InputEvent, cx| cx.notify()));
        subs.push(cx.subscribe(&db_search, |_, _, _: &InputEvent, cx| cx.notify()));
        for input in int.all().into_iter().chain(ns.inputs()).chain([&files_in.new_folder, &files_in.filter]) {
            subs.push(cx.subscribe(input, |_, _, _: &InputEvent, cx| cx.notify()));
        }
        subs.push(cx.subscribe(&vault_pw2, |_, _, _: &InputEvent, cx| cx.notify()));
        subs.push(cx.subscribe(&vault_pw, |this, _, e: &InputEvent, cx| {
            if matches!(e, InputEvent::PressEnter { .. }) {
                this.submit_vault(false, cx);
            }
        }));
        subs.push(cx.observe_window_appearance(window, |this, window, cx| {
            if this.theme_pref == ThemePref::System {
                theme::apply(theme::is_dark(ThemePref::System, Some(window)), this.translucent, cx);
                cx.notify();
            }
        }));

        // Key-file vaults open by themselves; password vaults wait for the user.
        if !secrets::is_unlocked() {
            if let Ok(sesame_core::vault::VaultMode::KeyFile) = sesame_core::vault::Vault::mode(&store) {
                if let Ok(path) = sesame_core::vault::Vault::key_file_path() {
                    if let Ok(v) = sesame_core::vault::Vault::unlock_with_key_file(store.clone(), &path) {
                        secrets::install(v);
                    }
                }
            }
        }
        let page = if !secrets::is_unlocked() {
            Page::Vault
        } else {
            match servers.first() {
                Some(s) => Page::Server(s.id, Section::Overview),
                None => Page::AddServer,
            }
        };
        let mut this = Self {
            store,
            servers,
            page,
            snapshots,
            busy: HashSet::new(),
            errors: HashMap::new(),
            notices: HashMap::new(),
            form,
            filter,
            site_search,
            site_kind: None,
            theme_pref,
            lang_pref,
            translucent,
            update: UpdateState::default(),
            pending: None,
            job: None,
            db_name,
            db_user,
            db_search,
            db_engine: None,
            db_filter: None,
            vault_pw,
            vault_pw2,
            vault_busy: false,
            vault_error: None,
            gh: Default::default(),
            cf: integrations::CfState::fresh(),
            int,
            ns,
            tunnels: HashMap::new(),
            files: None,
            files_in,
            terminal: None,
            _subscriptions: subs,
        };
        if this.page != Page::Vault && translucent {
            this.load_integrations(cx);
        }
        if translucent && this.page != Page::Vault && this.update_due() {
            this.check_updates(cx);
        }
        this
    }

    // ---- navigation & settings ------------------------------------------

    pub fn navigate(&mut self, page: Page, cx: &mut Context<Self>) {
        // Leaving the databases section closes the engine detail.
        if !matches!(page, Page::Server(_, Section::Databases)) {
            self.db_engine = None;
        }
        self.page = page;
        cx.notify();
    }

    /// Opens one database engine on the databases page (screenshots, demos).
    #[doc(hidden)]
    pub fn demo_open_engine(&mut self, kind: DatabaseKind, cx: &mut Context<Self>) {
        self.db_engine = Some(kind);
        cx.notify();
    }

    /// Selects a section of the current (or first) server; used by tests.
    pub fn show_section(&mut self, section: Section, cx: &mut Context<Self>) {
        let id = match self.page {
            Page::Server(id, _) => Some(id),
            _ => self.servers.first().map(|s| s.id),
        };
        if let Some(id) = id {
            self.navigate(Page::Server(id, section), cx);
        }
    }

    pub fn set_theme(&mut self, pref: ThemePref, window: &mut Window, cx: &mut Context<Self>) {
        self.theme_pref = pref;
        let _ = self.store.set_setting("theme", pref.as_str());
        theme::apply(theme::is_dark(pref, Some(window)), self.translucent, cx);
        cx.refresh_windows();
    }

    pub fn set_language(&mut self, pref: LangPref, window: &mut Window, cx: &mut Context<Self>) {
        self.lang_pref = pref;
        let _ = self.store.set_setting("language", pref.as_str());
        i18n::apply(pref);
        // Placeholders are baked into input state; refresh them.
        let fields = [
            (&self.filter, tr("nav.filter")),
            (&self.site_search, tr("sites.search")),
            (&self.db_search, tr("db.search")),
            (&self.db_user, tr("db.user_placeholder")),
            (&self.db_name, tr("db.name_placeholder")),
            (&self.int.repo_search, tr("repo.search")),
            (&self.int.zone_search, tr("cf.search_domains")),
            (&self.int.record_search, tr("cf.search_records")),
            (&self.form.key, tr("add.key_placeholder")),
            (&self.form.password, tr("add.password_placeholder")),
        ];
        for (input, text) in fields {
            input.update(cx, |s, cx| s.set_placeholder(text, window, cx));
        }
        cx.refresh_windows();
    }

    pub(crate) fn channel(&self) -> Channel {
        self.store
            .setting("update.channel")
            .ok()
            .flatten()
            .and_then(|c| Channel::parse(&c))
            .unwrap_or_else(|| Channel::default_for(&sesame_update::current_version()))
    }

    pub(crate) fn auto_update(&self) -> bool {
        self.store.setting_or("update.auto", "true") == "true"
    }

    // ---- updates ---------------------------------------------------------

    fn update_due(&self) -> bool {
        let last: i64 = self.store.setting_or("update.last_check", "0").parse().unwrap_or(0);
        now_secs() - last > 6 * 3600
    }

    pub(crate) fn check_updates(&mut self, cx: &mut Context<Self>) {
        if self.update.checking {
            return;
        }
        self.update.checking = true;
        self.update.error = None;
        cx.notify();
        let channel = self.channel();
        let job = runtime::get().spawn(async move { sesame_update::check(channel).await });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |this, cx| {
                this.update.checking = false;
                this.update.checked = true;
                let _ = this.store.set_setting("update.last_check", &now_secs().to_string());
                match result {
                    Ok(Ok(info)) => {
                        let skipped = this.store.setting("update.skip").ok().flatten();
                        this.update.info =
                            info.filter(|i| skipped.as_deref() != Some(i.latest.version.to_string().as_str()));
                        if this.update.info.is_some() && this.auto_update() && sesame_update::installed_location().is_some() {
                            this.install_update(cx);
                        }
                    }
                    Ok(Err(e)) => this.update.error = Some(format!("{e:#}")),
                    Err(e) => this.update.error = Some(e.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn install_update(&mut self, cx: &mut Context<Self>) {
        let Some(info) = self.update.info.clone() else { return };
        if self.update.downloading || self.update.ready.is_some() {
            return;
        }
        self.update.downloading = true;
        self.update.error = None;
        cx.notify();
        let job = runtime::get().spawn(async move {
            let dir = std::env::temp_dir().join("sesame-update");
            std::fs::create_dir_all(&dir)?;
            let archive = sesame_update::download(&info.latest, &dir).await?;
            sesame_update::install(&archive)
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |this, cx| {
                this.update.downloading = false;
                match result {
                    Ok(Ok(target)) => this.update.ready = Some(target),
                    Ok(Err(e)) => this.update.error = Some(format!("{e:#}")),
                    Err(e) => this.update.error = Some(e.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn restart_into_update(&mut self, cx: &mut Context<Self>) {
        if let Some(target) = &self.update.ready {
            if sesame_update::relaunch(target).is_ok() {
                cx.quit();
            }
        }
    }

    pub(crate) fn skip_update(&mut self, cx: &mut Context<Self>) {
        if let Some(info) = self.update.info.take() {
            let _ = self.store.set_setting("update.skip", &info.latest.version.to_string());
        }
        cx.notify();
    }

    // ---- admin panels -----------------------------------------------------

    /// Opens (or reuses) an SSH tunnel to a panel and shows it in the browser.
    pub(crate) fn open_panel(&mut self, server: ServerId, port: u16, cx: &mut Context<Self>) {
        if let Some(t) = self.tunnels.get(&(server, port)) {
            cx.open_url(&t.url("/"));
            return;
        }
        let Some(config) = self.servers.iter().find(|s| s.id == server).cloned() else { return };
        let job = runtime::get().spawn(async move { sesame_ssh::Tunnel::open(&config, port).await });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(t)) => {
                        cx.open_url(&t.url("/"));
                        this.tunnels.insert((server, port), t);
                    }
                    Ok(Err(e)) => {
                        this.errors.insert(server, trp("svc.tunnel_error", &[("error", &format!("{e:#}"))]));
                    }
                    Err(e) => {
                        this.errors.insert(server, e.to_string());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ---- vault ------------------------------------------------------------

    /// Creates or unlocks the vault. `no_password` picks the key-file mode on first run.
    pub(crate) fn submit_vault(&mut self, no_password: bool, cx: &mut Context<Self>) {
        use sesame_core::vault::{Vault, VaultMode};
        if self.vault_busy {
            return;
        }
        let mode = Vault::mode(&self.store).unwrap_or(VaultMode::New);
        let pw = self.vault_pw.read(cx).value().to_string();
        let pw2 = self.vault_pw2.read(cx).value().to_string();
        if mode == VaultMode::New && !no_password && pw != pw2 {
            self.vault_error = Some(tr("vault.mismatch"));
            cx.notify();
            return;
        }
        self.vault_busy = true;
        self.vault_error = None;
        cx.notify();
        let store = self.store.clone();
        // Argon2id takes a moment; keep the window responsive.
        let job = runtime::get().spawn_blocking(move || -> anyhow::Result<Vault> {
            match (mode, no_password) {
                (VaultMode::New, true) => Vault::create_with_key_file(store, &Vault::key_file_path()?),
                (VaultMode::New, false) => Vault::create_with_password(store, &pw),
                (VaultMode::Password, _) => Vault::unlock_with_password(store, &pw),
                (VaultMode::KeyFile, _) => Vault::unlock_with_key_file(store, &Vault::key_file_path()?),
            }
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |this, cx| {
                this.vault_busy = false;
                match result {
                    Ok(Ok(vault)) => {
                        secrets::install(vault);
                        if this.translucent {
                            this.load_integrations(cx);
                        }
                        this.page = match this.servers.first() {
                            Some(s) => Page::Server(s.id, Section::Overview),
                            None => Page::AddServer,
                        };
                        if this.translucent && this.update_due() {
                            this.check_updates(cx);
                        }
                    }
                    Ok(Err(e)) => this.vault_error = Some(e.to_string()),
                    Err(e) => this.vault_error = Some(e.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ---- servers ---------------------------------------------------------

    pub(crate) fn submit_add(&mut self, cx: &mut Context<Self>) {
        let name = self.form.name.read(cx).value().trim().to_string();
        let target = self.form.target.read(cx).value().trim().to_string();
        let key = self.form.key.read(cx).value().trim().to_string();
        let password = self.form.password.read(cx).value().to_string();
        let name = if name.is_empty() { target.split('@').next_back().unwrap_or("").to_string() } else { name };

        let mut server = match crate::cli::parse_target(&name, &target, (!key.is_empty()).then_some(key)) {
            Ok(s) => s,
            Err(e) => return self.form_error(e.to_string(), cx),
        };
        if !password.is_empty() {
            server.auth = AuthMethod::Password;
            if let Err(e) = secrets::set(server.id, SecretKind::Password, &password) {
                return self.form_error(format!("Vault: {e}"), cx);
            }
        }
        if let Err(e) = self.store.save_server(&server) {
            return self.form_error(e.to_string(), cx);
        }
        self.form.error = None;
        let id = server.id;
        self.servers.push(server);
        self.page = Page::Server(id, Section::Overview);
        self.discover(id, cx);
    }

    fn form_error(&mut self, e: String, cx: &mut Context<Self>) {
        self.form.error = Some(e);
        cx.notify();
    }

    pub(crate) fn discover(&mut self, id: ServerId, cx: &mut Context<Self>) {
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
                            this.notices.insert(id, trp("server.new_host_key", &[("fp", &fingerprint)]));
                        }
                    }
                    Ok(Err(e)) => {
                        this.errors.insert(id, format!("{e:#}"));
                    }
                    Err(e) => {
                        this.errors.insert(id, e.to_string());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn remove_server(&mut self, id: ServerId, cx: &mut Context<Self>) {
        let _ = self.store.delete_server(id);
        let _ = secrets::delete_server(id);
        self.servers.retain(|s| s.id != id);
        self.snapshots.remove(&id);
        self.page = match self.servers.first() {
            Some(s) => Page::Server(s.id, Section::Overview),
            None => Page::AddServer,
        };
        cx.notify();
    }

    fn current_server(&self) -> Option<&ServerConfig> {
        match self.page {
            Page::Server(id, _) => self.servers.iter().find(|s| s.id == id),
            _ => None,
        }
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---- rendering -------------------------------------------------------------

impl Render for SesameApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let g = Glass::get(cx).clone();
        if self.page == Page::Vault {
            return v_flex()
                .size_full()
                .bg(g.window)
                .text_color(g.text)
                .text_sm()
                .child(
                    div().absolute().size_full().bg(linear_gradient(
                        135.,
                        linear_color_stop(g.glow_a, 0.0),
                        linear_color_stop(g.glow_b, 1.0),
                    )),
                )
                .child(TitleBar::new().h(px(40.)).child(div()))
                .child(views::vault(self, cx))
                .into_any_element();
        }
        let content = match self.page {
            Page::Vault => div().into_any_element(),
            Page::Cloudflare => integrations::cloudflare_page(self, cx).into_any_element(),
            Page::AddServer => views::add_server(self, cx).into_any_element(),
            Page::Settings => views::settings(self, window, cx).into_any_element(),
            Page::Updates => views::updates(self, cx).into_any_element(),
            Page::Server(id, section) => match self.servers.iter().find(|s| s.id == id).cloned() {
                Some(server) => views::server(self, &server, section, window, cx).into_any_element(),
                None => empty(tr("server.empty"), cx).into_any_element(),
            },
        };

        v_flex()
            .size_full()
            .bg(g.window)
            .text_color(g.text)
            .text_sm()
            .font_family(".SystemUIFont")
            .child(
                // Soft glows behind everything.
                div()
                    .absolute()
                    .size_full()
                    .bg(linear_gradient(
                        135.,
                        linear_color_stop(g.glow_a, 0.0),
                        linear_color_stop(g.glow_b, 1.0),
                    ))
                    .opacity(0.9),
            )
            .child(self.render_titlebar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_sidebar(cx))
                    .child(
                        div()
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .pr_2()
                            .pb_2()
                            .child(
                                panel(cx)
                                    .size_full()
                                    .overflow_hidden()
                                    .child(div().flex_1().min_h_0().child(content))
                                    .children(self.render_job(cx)),
                            ),
                    ),
            )
            .child(self.render_statusbar(cx))
            .children(self.render_confirm(cx))
            .into_any_element()
    }
}

impl SesameApp {
    fn render_titlebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let g = Glass::get(cx).clone();
        let mut crumbs = h_flex().gap_1p5().items_center().text_color(g.dim).child(
            img(ImageSource::Resource(Resource::Embedded("logo.svg".into()))).size(px(16.)).rounded_sm(),
        );
        crumbs = crumbs.child(div().font_weight(FontWeight::SEMIBOLD).text_color(g.text).child("Sesame"));
        let sep = || div().text_color(g.faint).child("/");
        match self.page {
            Page::Server(_, section) => {
                if let Some(s) = self.current_server() {
                    crumbs = crumbs.child(sep()).child(s.name.clone()).child(sep()).child(section.label());
                    if let (Section::Site(i) | Section::Files(i), Some(snap)) = (section, self.snapshots.get(&s.id)) {
                        if let Some(site) = snap.sites.get(i) {
                            crumbs = crumbs.child(sep()).child(site.domains.first().cloned().unwrap_or_default());
                        }
                    }
                }
            }
            Page::AddServer => crumbs = crumbs.child(sep()).child(tr("nav.add_server")),
            Page::Settings => crumbs = crumbs.child(sep()).child(tr("nav.settings")),
            Page::Updates => crumbs = crumbs.child(sep()).child(tr("nav.updates")),
            Page::Vault => {}
            Page::Cloudflare => crumbs = crumbs.child(sep()).child("Cloudflare"),
        }

        let theme_icon = match self.theme_pref {
            ThemePref::System => IconName::Monitor,
            ThemePref::Light => IconName::Sun,
            ThemePref::Dark => IconName::Moon,
        };
        let lang = i18n::LangPref::parse(rust_i18n::locale().as_ref());
        let tool = |id: &'static str| {
            div()
                .id(id)
                .px_1p5()
                .py_1()
                .rounded_md()
                .cursor_pointer()
                .hover(|d| d.bg(g.panel_hover))
        };
        let mut right = h_flex().gap_1().pr_2().items_center();
        if let Some(info) = &self.update.info {
            right = right.child(
                tool("tb-update")
                    .child(pill(trp("update.available", &[("version", &info.latest.version.to_string())]), g.accent))
                    .on_click(cx.listener(|this, _, _, cx| this.navigate(Page::Updates, cx))),
            );
        }
        right = right
            .child(
                tool("tb-lang")
                    .child(div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(g.dim).child(match lang {
                        LangPref::Ru => "RU",
                        LangPref::Az => "AZ",
                        _ => "EN",
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let next = match lang {
                            LangPref::Ru => LangPref::Az,
                            LangPref::Az => LangPref::En,
                            _ => LangPref::Ru,
                        };
                        this.set_language(next, window, cx);
                    })),
            )
            .child(tool("tb-theme").child(icon(theme_icon, g.dim)).on_click(cx.listener(|this, _, window, cx| {
                let next = match this.theme_pref {
                    ThemePref::System => ThemePref::Light,
                    ThemePref::Light => ThemePref::Dark,
                    ThemePref::Dark => ThemePref::System,
                };
                this.set_theme(next, window, cx);
            })))
            .child(
                tool("tb-settings")
                    .child(icon(IconName::Settings, g.dim))
                    .on_click(cx.listener(|this, _, _, cx| this.navigate(Page::Settings, cx))),
            );

        TitleBar::new()
            .h(px(40.))
            .child(crumbs)
            .child(right)
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let g = Glass::get(cx).clone();
        let q = self.filter.read(cx).value().to_lowercase();
        let mut tree = v_flex().gap_0p5();

        for s in &self.servers {
            let id = s.id;
            let snap = self.snapshots.get(&id);
            let matching_sites: Vec<(usize, &Site)> = snap
                .map(|sn| {
                    sn.sites
                        .iter()
                        .enumerate()
                        .filter(|(_, site)| !q.is_empty() && site.domains.iter().any(|d| d.to_lowercase().contains(&q)))
                        .collect()
                })
                .unwrap_or_default();
            let server_match = q.is_empty() || s.name.to_lowercase().contains(&q) || s.host.contains(&q);
            if !server_match && matching_sites.is_empty() {
                continue;
            }
            let selected = matches!(self.page, Page::Server(sid, _) if sid == id);
            let status = if self.busy.contains(&id) {
                g.warn
            } else if self.errors.contains_key(&id) {
                g.bad
            } else if snap.is_some() {
                g.ok
            } else {
                g.faint
            };
            tree = tree.child(
                h_flex()
                    .id(SharedString::from(format!("srv-{id}")))
                    .gap_2()
                    .px_2()
                    .h(px(30.))
                    .rounded_md()
                    .cursor_pointer()
                    .when(selected, |d| d.bg(g.panel_hover))
                    .hover(|d| d.bg(g.panel_hover))
                    .on_click(cx.listener(move |this, _, _, cx| this.navigate(Page::Server(id, Section::Overview), cx)))
                    .child(icon(IconName::Server, if selected { g.accent } else { g.dim }))
                    .child(div().flex_1().min_w_0().overflow_hidden().font_weight(FontWeight::MEDIUM).child(s.name.clone()))
                    .child(dot(status)),
            );

            // Sections of the selected server, like a project tree.
            if selected && q.is_empty() {
                let current = match self.page {
                    Page::Server(_, sec) => sec,
                    _ => Section::Overview,
                };
                for section in Section::NAV {
                    let active = section == current || (section == Section::Sites && matches!(current, Section::Site(_) | Section::NewSite | Section::Files(_)));
                    let count = snap.and_then(|sn| match section {
                        Section::Sites => Some(sn.sites.iter().filter(|s| s.domains != ["*"]).count()),
                        Section::Processes => Some(sn.supervisor.len()),
                        Section::Databases => Some(sn.databases.len()),
                        _ => None,
                    });
                    tree = tree.child(
                        h_flex()
                            .id(SharedString::from(format!("nav-{}", section.slug())))
                            .gap_2()
                            .pl(px(26.))
                            .pr_2()
                            .h(px(28.))
                            .rounded_md()
                            .cursor_pointer()
                            .when(active, |d| d.bg(g.accent_soft).text_color(g.text))
                            .when(!active, |d| d.text_color(g.dim))
                            .hover(|d| d.bg(g.panel_hover))
                            .on_click(cx.listener(move |this, _, _, cx| this.navigate(Page::Server(id, section), cx)))
                            .child(icon(section.icon(), if active { g.accent } else { g.faint }))
                            .child(div().flex_1().child(section.label()))
                            .when_some(count, |d, n| d.child(div().text_xs().text_color(g.faint).child(n.to_string()))),
                    );
                }
            }
            for (i, site) in matching_sites {
                tree = tree.child(
                    h_flex()
                        .id(SharedString::from(format!("hit-{id}-{i}")))
                        .gap_2()
                        .pl(px(26.))
                        .h(px(28.))
                        .rounded_md()
                        .cursor_pointer()
                        .text_color(g.dim)
                        .hover(|d| d.bg(g.panel_hover))
                        .on_click(cx.listener(move |this, _, _, cx| this.navigate(Page::Server(id, Section::Site(i)), cx)))
                        .child(icon(IconName::Globe, g.faint))
                        .child(site.domains.first().cloned().unwrap_or_default()),
                );
            }
        }

        v_flex()
            .w(px(248.))
            .h_full()
            .flex_shrink_0()
            .px_2()
            .pb_2()
            .gap_2()
            .child(Input::new(&self.filter).prefix(icon(IconName::Search, g.faint)).cleanable(true))
            .child(
                h_flex()
                    .px_2()
                    .pt_1()
                    .justify_between()
                    .child(heading(&tr("nav.servers"), cx))
                    .child(
                        div()
                            .id("add-server")
                            .p_1()
                            .rounded_md()
                            .cursor_pointer()
                            .hover(|d| d.bg(g.panel_hover))
                            .child(icon(IconName::Plus, g.dim))
                            .on_click(cx.listener(|this, _, _, cx| this.navigate(Page::AddServer, cx))),
                    ),
            )
            .child(div().id("tree").flex_1().overflow_y_scroll().child(tree).child(integrations::sidebar_entry(self, cx)))
    }

    fn render_statusbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let g = Glass::get(cx).clone();
        let mut left = h_flex().gap_3().items_center();
        if let Some(s) = self.current_server() {
            let snap = self.snapshots.get(&s.id);
            let color = if self.busy.contains(&s.id) {
                g.warn
            } else if self.errors.contains_key(&s.id) {
                g.bad
            } else if snap.is_some() {
                g.ok
            } else {
                g.faint
            };
            left = left.child(h_flex().gap_1p5().items_center().child(dot(color)).child(mono(s.address())));
            match snap {
                Some(sn) => {
                    left = left
                        .child(trp("server.discovered_at", &[("time", &views::short_time(&sn.taken_at))]))
                        .child(trp("status.sites", &[("n", &sn.sites.iter().filter(|s| s.domains != ["*"]).count().to_string())]))
                        .child(trp("status.processes", &[("n", &sn.supervisor.len().to_string())]));
                }
                None => left = left.child(tr("status.not_connected")),
            }
        }
        let mut right = h_flex().gap_3().items_center();
        if self.update.downloading {
            right = right.child(tr("update.downloading"));
        } else if self.update.ready.is_some() {
            right = right.child(
                div()
                    .id("sb-restart")
                    .cursor_pointer()
                    .child(pill(tr("update.restart"), g.accent))
                    .on_click(cx.listener(|this, _, _, cx| this.restart_into_update(cx))),
            );
        }
        right = right.child(mono(format!("v{}", env!("CARGO_PKG_VERSION"))));
        h_flex()
            .h(px(28.))
            .px_3()
            .justify_between()
            .text_xs()
            .text_color(g.dim)
            .child(left)
            .child(right)
    }
}
