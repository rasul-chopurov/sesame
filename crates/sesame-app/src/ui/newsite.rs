//! The "new site" wizard page, and the Site services / Subdomains cards.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use sesame_core::snapshot::{DatabaseKind, Framework, ProcessRole, ServerSnapshot, Site};
use sesame_core::{ServerId, secrets};
use sesame_ops::Plan;
use sesame_ops::site::{NewSite, Runtime, SiteKind, Source};

use super::theme::Glass;
use super::widgets::*;
use super::{Page, Section, SesameApp};
use crate::i18n::{tr, trp};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Code {
    Skeleton,
    GitHub,
}

pub struct Form {
    pub domain: Entity<InputState>,
    pub repo: Entity<InputState>,
    pub branch: Entity<InputState>,
    pub root: Entity<InputState>,
    pub sub: Entity<InputState>,
    pub kind: SiteKind,
    pub runtime: Runtime,
    pub code: Code,
    pub database: Option<DatabaseKind>,
    pub redis: bool,
    pub queue: bool,
    pub scheduler: bool,
    pub error: Option<String>,
}

impl Form {
    pub fn new(window: &mut Window, cx: &mut Context<SesameApp>) -> Self {
        let input = |ph: &str, window: &mut Window, cx: &mut Context<SesameApp>| {
            let ph = ph.to_string();
            cx.new(|cx| InputState::new(window, cx).placeholder(ph))
        };
        Self {
            domain: input("shop.example.com", window, cx),
            repo: input("owner/name", window, cx),
            branch: input("main", window, cx),
            root: input("public", window, cx),
            sub: input("api", window, cx),
            kind: SiteKind::Laravel,
            runtime: Runtime::Fpm,
            code: Code::Skeleton,
            database: None,
            redis: false,
            queue: false,
            scheduler: true,
            error: None,
        }
    }

    pub fn inputs(&self) -> [&Entity<InputState>; 5] {
        [&self.domain, &self.repo, &self.branch, &self.root, &self.sub]
    }
}

fn installed(snap: &ServerSnapshot, kind: DatabaseKind) -> bool {
    snap.databases.iter().any(|d| d.kind == kind)
}

impl SesameApp {
    /// Opens the wizard, optionally with a domain filled in (subdomains).
    pub(crate) fn open_new_site(&mut self, server: ServerId, domain: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(d) = domain {
            self.ns.domain.update(cx, |s, cx| s.set_value(d, window, cx));
        }
        self.ns.error = None;
        self.navigate(Page::Server(server, Section::NewSite), cx);
    }

    fn build_new_site(&self, server: ServerId, snap: &ServerSnapshot, cx: &App) -> anyhow::Result<Plan> {
        let domain = self.ns.domain.read(cx).value().trim().to_lowercase();
        let source = (self.ns.code == Code::GitHub).then(|| Source {
            repo: self.ns.repo.read(cx).value().trim().to_string(),
            branch: {
                let b = self.ns.branch.read(cx).value().trim().to_string();
                if b.is_empty() { "main".into() } else { b }
            },
        });
        let root = self.ns.root.read(cx).value().trim().to_string();
        let site = NewSite {
            domain: domain.clone(),
            aliases: vec![],
            kind: self.ns.kind,
            runtime: self.ns.runtime,
            source,
            web_root: if root.is_empty() {
                if self.ns.kind == SiteKind::Laravel { "public".into() } else { ".".into() }
            } else {
                root
            },
            database: if self.ns.kind == SiteKind::Laravel { self.ns.database } else { None },
            scheduler: self.ns.kind == SiteKind::Laravel && self.ns.scheduler,
        };
        let mut plan = sesame_ops::site::new_site(&site, snap)?;
        let (_, app) = sesame_ops::site::site_paths(&domain);
        if self.ns.kind == SiteKind::Laravel && self.ns.redis {
            let admin = secrets::get_named(server, "REDIS_PASSWORD").ok().flatten();
            let r = sesame_ops::site::redis_for_site(&domain, &app, admin.as_deref())?;
            plan.steps.extend(r.steps);
            plan.details.extend(r.details);
            plan.reveal.extend(r.reveal);
        }
        if self.ns.kind == SiteKind::Laravel && self.ns.queue {
            let conn = if self.ns.redis { "redis" } else { "database" };
            let q = sesame_ops::site::queue_for_site(&domain, &app, conn, 1)?;
            plan.steps.extend(q.steps);
            plan.details.extend(q.details);
        }
        Ok(plan)
    }

    fn submit_new_site(&mut self, server: ServerId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(snap) = self.snapshots.get(&server).cloned() else { return };
        match self.build_new_site(server, &snap, cx) {
            Ok(plan) => {
                self.ns.error = None;
                self.request(server, Ok(plan), None, window, cx);
            }
            Err(e) => {
                self.ns.error = Some(e.to_string());
                cx.notify();
            }
        }
    }
}

fn choice<T: Copy + PartialEq + 'static>(
    id: &str,
    options: Vec<(T, String, bool)>,
    selected: T,
    on_pick: impl Fn(&mut SesameApp, T) + 'static,
    cx: &mut Context<SesameApp>,
) -> Div {
    let g = Glass::get(cx).clone();
    let on_pick = std::rc::Rc::new(on_pick);
    let mut row = h_flex().p_0p5().gap_0p5().rounded_lg().bg(g.panel).border_1().border_color(g.line).flex_wrap();
    for (i, (value, label, enabled)) in options.into_iter().enumerate() {
        let active = value == selected;
        let pick = on_pick.clone();
        row = row.child(
            div()
                .id(SharedString::from(format!("{id}-{i}")))
                .px_3()
                .py_1()
                .rounded_md()
                .text_sm()
                .when(!enabled, |d| d.text_color(g.faint).opacity(0.6))
                .when(enabled, |d| {
                    d.cursor_pointer()
                        .when(active, |d| d.bg(g.accent).text_color(g.accent_text))
                        .when(!active, |d| d.text_color(g.dim).hover(|d| d.bg(g.panel_hover)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            pick(this, value);
                            cx.notify();
                        }))
                })
                .child(label),
        );
    }
    row
}

pub fn wizard(app: &SesameApp, server: ServerId, snap: &ServerSnapshot, cx: &mut Context<SesameApp>) -> Div {
    let g = Glass::get(cx).clone();
    let f = &app.ns;
    let row = |label: String, control: AnyElement| {
        h_flex().gap_4().min_h(px(40.)).child(div().w(px(160.)).flex_shrink_0().text_color(g.dim).child(label)).child(div().flex_1().child(control))
    };
    let laravel = f.kind == SiteKind::Laravel;
    let mut dbs = vec![(None, tr("ns.none"), true)];
    for (k, name) in [(DatabaseKind::MySql, "MySQL"), (DatabaseKind::MariaDb, "MariaDB"), (DatabaseKind::Postgres, "PostgreSQL")] {
        dbs.push((Some(k), name.to_string(), installed(snap, k)));
    }
    let any_db = dbs.iter().skip(1).any(|(_, _, ok)| *ok);
    let redis_ok = installed(snap, DatabaseKind::Redis);

    let mut form = card(&tr("ns.title"), cx)
        .gap_3()
        .child(div().text_color(g.dim).child(tr("ns.lead")))
        .child(row(tr("ns.domain"), div().w(px(380.)).child(Input::new(&f.domain)).into_any_element()))
        .child(row(
            tr("ns.kind"),
            choice("kind", vec![(SiteKind::Laravel, "Laravel".into(), true), (SiteKind::Static, tr("ns.static"), true)], f.kind, |a, v| a.ns.kind = v, cx).into_any_element(),
        ))
        .child(row(
            tr("ns.source"),
            choice(
                "code",
                vec![
                    (Code::Skeleton, if laravel { tr("ns.skeleton") } else { tr("ns.placeholder") }, true),
                    (Code::GitHub, tr("ns.github"), true),
                ],
                f.code,
                |a, v| a.ns.code = v,
                cx,
            )
            .into_any_element(),
        ));
    if f.code == Code::GitHub {
        form = form
            .child(row(
                String::new(),
                h_flex()
                    .gap_2()
                    .children(brand("github", px(16.)))
                    .child(div().w(px(260.)).child(Input::new(&f.repo)))
                    .child(div().w(px(140.)).child(Input::new(&f.branch)))
                    .into_any_element(),
            ))
            .child(row(String::new(), div().text_xs().text_color(g.faint).child(tr("ns.repo_hint")).into_any_element()));
    }
    form = form.child(row(tr("ns.root"), div().w(px(200.)).child(Input::new(&f.root)).into_any_element()));
    if laravel {
        form = form.child(row(
            tr("ns.runtime"),
            choice("runtime", vec![(Runtime::Fpm, tr("ns.fpm"), true), (Runtime::Octane, tr("ns.octane"), true)], f.runtime, |a, v| a.ns.runtime = v, cx).into_any_element(),
        ));
    }

    let mut services = card(&tr("ns.services"), cx).gap_3();
    if laravel {
        services = services
            .child(row(tr("ns.database"), choice("db", dbs, f.database, |a, v| a.ns.database = v, cx).into_any_element()))
            .when(!any_db, |d| d.child(div().text_xs().text_color(g.faint).child(trp("ns.not_installed", &[("what", "MySQL / MariaDB / PostgreSQL")]))))
            .child(row(
                String::new(),
                v_flex()
                    .gap_2()
                    .child(
                        Switch::new("ns-redis")
                            .checked(f.redis && redis_ok)
                            .disabled(!redis_ok)
                            .label(tr("ns.redis"))
                            .on_click(cx.listener(|this, v: &bool, _, cx| {
                                this.ns.redis = *v;
                                cx.notify();
                            })),
                    )
                    .when(!redis_ok, |d| d.child(div().text_xs().text_color(g.faint).child(trp("ns.not_installed", &[("what", "Redis")]))))
                    .child(Switch::new("ns-queue").checked(f.queue).label(tr("ns.queue")).on_click(cx.listener(|this, v: &bool, _, cx| {
                        this.ns.queue = *v;
                        cx.notify();
                    })))
                    .child(Switch::new("ns-scheduler").checked(f.scheduler).label(tr("ns.scheduler")).on_click(cx.listener(|this, v: &bool, _, cx| {
                        this.ns.scheduler = *v;
                        cx.notify();
                    })))
                    .into_any_element(),
            ));
    } else {
        services = services.child(div().text_color(g.faint).child("—"));
    }

    v_flex()
        .gap_4()
        .max_w(px(900.))
        .child(
            h_flex()
                .id("ns-back")
                .gap_1()
                .cursor_pointer()
                .text_color(g.dim)
                .hover(|d| d.text_color(g.text))
                .on_click(cx.listener(move |this, _, _, cx| this.navigate(Page::Server(server, Section::Sites), cx)))
                .child("←")
                .child(tr("sites.back")),
        )
        .child(form)
        .child(services)
        .when_some(f.error.clone(), |d, e| d.child(div().text_color(g.bad).child(e)))
        .child(
            h_flex().child(
                Button::new("ns-create")
                    .primary()
                    .icon(IconName::Rocket)
                    .label(tr("ns.create"))
                    .on_click(cx.listener(move |this, _, window, cx| this.submit_new_site(server, window, cx))),
            ),
        )
}

/// What the site already has and buttons to add the rest.
pub fn site_services_card(_app: &SesameApp, server: ServerId, site: &Site, snap: &ServerSnapshot, cx: &mut Context<SesameApp>) -> Div {
    let g = Glass::get(cx).clone();
    let mut c = card(&tr("ns.services"), cx);
    let (Some(app_path), Some(domain)) = (site.app_path.clone(), site.domains.first().cloned()) else { return c };
    if site.framework != Framework::Laravel {
        return c.child(div().text_xs().text_color(g.faint).child("Laravel only"));
    }
    let env = site.env.clone().unwrap_or_default();
    let procs: Vec<_> = snap.supervisor.iter().filter(|p| site.processes.contains(&p.name)).collect();
    let row = |label: String, state: Option<String>, action: Option<Button>| {
        h_flex()
            .gap_3()
            .min_h(px(32.))
            .child(div().w(px(160.)).text_color(g.dim).child(label))
            .child(div().flex_1().child(match state {
                Some(s) => pill(s, g.ok),
                None => pill(tr("ns.none"), g.faint),
            }))
            .children(action)
    };

    // Database
    let db_state = env.db_database.clone().map(|d| format!("{} · {d}", env.db_connection.clone().unwrap_or_default()));
    let db_action = if db_state.is_none() {
        snap.databases.iter().find(|d| matches!(d.kind, DatabaseKind::MySql | DatabaseKind::MariaDb | DatabaseKind::Postgres)).map(|d| {
            let kind = d.kind;
            let (dm, ap) = (domain.clone(), app_path.clone());
            Button::new("svc-site-db").xsmall().primary().icon(IconName::Plus).label(tr("svc_site.add")).on_click(cx.listener(move |this, _, window, cx| {
                this.request(server, sesame_ops::site::database_for_site(&dm, &ap, kind, true), None, window, cx);
            }))
        })
    } else {
        None
    };
    c = c.child(row(tr("ns.database"), db_state, db_action));

    // Redis
    let redis_state = env.redis_username.clone().map(|u| format!("ACL {u}"));
    let redis_action = (redis_state.is_none() && installed(snap, DatabaseKind::Redis)).then(|| {
        let (dm, ap) = (domain.clone(), app_path.clone());
        Button::new("svc-site-redis").xsmall().primary().icon(IconName::Plus).label(tr("svc_site.add")).on_click(cx.listener(move |this, _, window, cx| {
            let admin = secrets::get_named(server, "REDIS_PASSWORD").ok().flatten();
            this.request(server, sesame_ops::site::redis_for_site(&dm, &ap, admin.as_deref()), None, window, cx);
        }))
    });
    c = c.child(row("Redis".into(), redis_state, redis_action));

    // Queue
    let workers = procs.iter().filter(|p| p.role == ProcessRole::Queue).count();
    let queue_action = (workers == 0).then(|| {
        let (dm, ap) = (domain.clone(), app_path.clone());
        let conn = if env.redis_username.is_some() || env.queue_connection.as_deref() == Some("redis") { "redis" } else { "database" };
        Button::new("svc-site-queue").xsmall().primary().icon(IconName::Plus).label(tr("svc_site.add")).on_click(cx.listener(move |this, _, window, cx| {
            this.request(server, sesame_ops::site::queue_for_site(&dm, &ap, conn, 1), None, window, cx);
        }))
    });
    c = c.child(row(tr("ns.queue"), (workers > 0).then(|| trp("svc_site.workers", &[("n", &workers.to_string())])), queue_action));

    // Reverb
    let reverb = procs.iter().any(|p| p.role == ProcessRole::Reverb);
    let reverb_action = (!reverb).then(|| {
        let s2 = site.clone();
        let port = (8080u16..8200).find(|p| !snap.ports.iter().any(|x| x.port == *p)).unwrap_or(8080);
        Button::new("svc-site-reverb").xsmall().primary().icon(IconName::Plus).label(tr("svc_site.add")).on_click(cx.listener(move |this, _, window, cx| {
            this.request(server, sesame_provision::reverb_for_site(&s2, port), None, window, cx);
        }))
    });
    c = c.child(row("Reverb".into(), reverb.then(|| tr("svc_site.on")), reverb_action));

    // Scheduler
    let scheduled = snap.cron.iter().any(|e| e.command.contains("schedule:run") && e.command.contains(&app_path));
    let sched_action = (!scheduled).then(|| {
        let (dm, ap) = (domain.clone(), app_path.clone());
        Button::new("svc-site-cron").xsmall().primary().icon(IconName::Plus).label(tr("svc_site.add")).on_click(cx.listener(move |this, _, window, cx| {
            this.request(server, sesame_ops::site::scheduler_for_site(&dm, &ap), None, window, cx);
        }))
    });
    c.child(row(tr("ns.scheduler"), scheduled.then(|| tr("svc_site.on")), sched_action))
}

/// Subdomains of a site (separate isolated sites) and adding one.
pub fn subdomains_card(app: &SesameApp, server: ServerId, site: &Site, snap: &ServerSnapshot, cx: &mut Context<SesameApp>) -> Div {
    let g = Glass::get(cx).clone();
    let Some(parent) = site.domains.iter().find(|d| *d != "*").cloned() else { return div() };
    let mut c = card(&tr("sub.title"), cx).child(div().text_xs().text_color(g.faint).child(tr("sub.hint")));
    let subs: Vec<(usize, &Site)> = snap
        .sites
        .iter()
        .enumerate()
        .filter(|(_, s)| s.domains.iter().any(|d| d.ends_with(&format!(".{parent}")) && !site.domains.contains(d)))
        .collect();
    if subs.is_empty() {
        c = c.child(div().text_color(g.dim).child(tr("sub.none")));
    }
    for (i, s) in subs {
        c = c.child(
            h_flex()
                .id(SharedString::from(format!("sub-{i}")))
                .gap_2()
                .min_h(px(30.))
                .px_2()
                .rounded_md()
                .cursor_pointer()
                .hover(|d| d.bg(g.panel_hover))
                .on_click(cx.listener(move |this, _, _, cx| this.navigate(Page::Server(server, Section::Site(i)), cx)))
                .child(icon(IconName::Globe, g.faint))
                .child(mono(s.domains.first().cloned().unwrap_or_default()))
                .child(div().text_xs().text_color(g.faint).child(s.app_path.clone().unwrap_or_default())),
        );
    }
    let parent2 = parent.clone();
    c.child(
        h_flex()
            .gap_2()
            .child(div().w(px(160.)).child(Input::new(&app.ns.sub)))
            .child(div().text_color(g.dim).child(format!(".{parent}")))
            .child(
                Button::new("sub-add")
                    .primary()
                    .small()
                    .icon(IconName::Plus)
                    .label(tr("sub.add"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let sub = this.ns.sub.read(cx).value().trim().to_lowercase();
                        if sub.is_empty() {
                            return;
                        }
                        this.open_new_site(server, Some(format!("{sub}.{parent2}")), window, cx);
                    })),
            ),
    )
}
