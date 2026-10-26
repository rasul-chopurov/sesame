//! Page and section views.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use sesame_core::{ServerConfig, Store as _};
use sesame_core::snapshot::*;
use sesame_discovery::insights::{self, Level};
use sesame_update::Channel;

use super::theme::{Glass, ThemePref};
use super::widgets::*;
use super::{Page, Section, SesameApp};
use crate::format;
use crate::i18n::{LangPref, tr, trp};

/// `2026-10-26T08:47:42Z` → `2026-10-26 08:47`.
pub fn short_time(rfc3339: &str) -> String {
    rfc3339.replace('T', " ").chars().take(16).collect()
}

fn page(cx: &App) -> Div {
    let _ = cx;
    v_flex().size_full()
}

fn scroll(id: &'static str, body: impl IntoElement) -> Stateful<Div> {
    div().id(id).flex_1().min_h_0().overflow_y_scroll().p_5().child(body)
}

// ---- server ----------------------------------------------------------------

pub fn server(
    app: &SesameApp,
    server: &ServerConfig,
    section: Section,
    _window: &mut Window,
    cx: &mut Context<SesameApp>,
) -> impl IntoElement {
    let g = Glass::get(cx).clone();
    let id = server.id;
    let busy = app.busy.contains(&id);
    let snap = app.snapshots.get(&id);

    let header = h_flex()
        .px_5()
        .py_3()
        .gap_3()
        .border_b_1()
        .border_color(g.line)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().text_lg().font_weight(FontWeight::BOLD).child(section.label()))
                .child(div().text_xs().text_color(g.dim).child(format!(
                    "{} · {}",
                    server.name,
                    snap.map(|s| s.os.distro.clone()).unwrap_or_else(|| server.address())
                ))),
        )
        .child(
            Button::new("discover")
                .primary()
                .loading(busy)
                .icon(IconName::RefreshCw)
                .label(if busy {
                    tr("server.discovering")
                } else if snap.is_some() {
                    tr("server.rediscover")
                } else {
                    tr("server.discover")
                })
                .on_click(cx.listener(move |this, _, _, cx| this.discover(id, cx))),
        )
        .child(
            Button::new("remove")
                .ghost()
                .label(tr("server.remove"))
                .on_click(cx.listener(move |this, _, _, cx| this.remove_server(id, cx))),
        );

    let mut body = v_flex().gap_4();
    if let Some(e) = app.errors.get(&id) {
        body = body.child(card(&tr("server.could_not_discover"), cx).child(div().text_color(g.bad).child(e.clone())));
    }
    if let Some(n) = app.notices.get(&id) {
        body = body.child(div().text_xs().text_color(g.dim).child(n.clone()));
    }
    body = match snap {
        None if busy => body.child(empty(tr("server.connecting"), cx)),
        None => body.child(empty(tr("server.not_discovered"), cx)),
        Some(s) => match section {
            Section::Overview => body.child(overview(s, cx)),
            Section::Sites => body.child(sites(app, id, s, cx)),
            Section::Site(i) => body.child(site_detail(id, s, i, cx)),
            Section::Processes => body.child(processes(s, cx)),
            Section::Services => body.child(services(s, cx)),
            Section::Databases => body.child(databases(s, cx)),
            Section::MailCron => body.child(mail_cron(s, cx)),
            Section::Security => body.child(security(s, cx)),
        },
    };
    page(cx).child(header).child(scroll("content", body))
}

// ---- overview --------------------------------------------------------------

fn overview(s: &ServerSnapshot, cx: &App) -> Div {
    let g = Glass::get(cx);
    let os = &s.os;
    let mem_used = os.mem_total_bytes.saturating_sub(os.mem_available_bytes);

    let mut attention = card(&tr("overview.attention"), cx);
    let list = insights::insights(s, time::OffsetDateTime::now_utc());
    if list.is_empty() && s.warnings.is_empty() {
        attention = attention.child(h_flex().gap_2().child(icon(IconName::Check, g.ok)).child(tr("overview.all_good")));
    }
    for i in &list {
        let (color, label, ic) = match i.level {
            Level::Critical => (g.bad, tr("level.critical"), IconName::CircleAlert),
            Level::Warning => (g.warn, tr("level.warning"), IconName::TriangleAlert),
            Level::Info => (g.info, tr("level.info"), IconName::Info),
        };
        let params: Vec<(&str, &str)> = i.params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        attention = attention.child(
            h_flex()
                .gap_2()
                .min_h(px(26.))
                .child(icon(ic, color))
                .child(div().w(px(84.)).child(pill(label, color)))
                .child(div().flex_1().child(trp(&format!("insight.{}", i.code), &params))),
        );
    }
    for w in &s.warnings {
        attention = attention.child(h_flex().gap_2().child(pill(tr("level.partial"), g.dim)).child(w.clone()));
    }

    let meter = |label: String, used: u64, total: u64| {
        let pct = format::percent(used, total);
        let color = if pct >= 90 { g.bad } else if pct >= 75 { g.warn } else { g.accent };
        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .justify_between()
                    .child(div().text_color(g.dim).child(label))
                    .child(trp(
                        "overview.of",
                        &[("used", &format::bytes(used)), ("total", &format::bytes(total)), ("pct", &pct.to_string())],
                    )),
            )
            .child(
                div().h(px(6.)).w_full().rounded_full().bg(g.line).child(
                    div().h_full().rounded_full().bg(color).w(relative(pct as f32 / 100.0)),
                ),
            )
    };

    let stack_row = |slug: Option<&str>, label: String, value: String| {
        h_flex()
            .gap_2()
            .min_h(px(26.))
            .child(div().w(px(18.)).children(slug.and_then(|s| brand(s, px(15.)))))
            .child(div().w(px(110.)).text_color(g.dim).child(label))
            .child(div().flex_1().min_w_0().overflow_hidden().child(value))
    };
    let mut stack = card(&tr("overview.stack"), cx);
    for w in &s.web_servers {
        let slug = match w.kind {
            WebServerKind::Caddy | WebServerKind::FrankenPhp => "caddy",
            WebServerKind::Nginx => "nginx",
            WebServerKind::Apache => "apache",
        };
        stack = stack.child(stack_row(Some(slug), tr("overview.web"), w.version.clone().unwrap_or_else(|| format!("{:?}", w.kind))));
    }
    for p in &s.php {
        stack = stack.child(stack_row(Some("php"), "PHP".into(), p.version.clone()));
    }
    for d in &s.databases {
        stack = stack.child(stack_row(Some(db_slug(d.kind)), db_name(d.kind).into(), d.version.clone().unwrap_or_default()));
    }

    v_flex()
        .gap_4()
        .child(attention)
        .child(
            div()
                .grid()
                .grid_cols(2)
                .gap_4()
                .child(
                    card(&tr("overview.machine"), cx)
                        .child(kv(&tr("overview.hostname"), os.hostname.clone(), cx))
                        .child(kv(&tr("overview.system"), os.distro.clone(), cx))
                        .child(kv(&tr("overview.kernel"), format!("{} ({})", os.kernel, os.arch), cx))
                        .child(kv(&tr("overview.cpus"), os.cpus.to_string(), cx))
                        .child(kv(&tr("overview.uptime"), format::duration(os.uptime_seconds), cx))
                        .child(kv(
                            &tr("overview.access"),
                            if os.privileged { tr("overview.access_root") } else { tr("overview.access_limited") },
                            cx,
                        )),
                )
                .child(
                    card(&tr("overview.resources"), cx)
                        .gap_3()
                        .child(meter(tr("overview.memory"), mem_used, os.mem_total_bytes))
                        .child(meter(tr("overview.disk"), os.disk_used_bytes, os.disk_total_bytes))
                        .child(kv(&tr("overview.load"), format!("{:.2}  {:.2}  {:.2}", os.load[0], os.load[1], os.load[2]), cx)),
                )
                .child(stack)
                .child(
                    card(&tr("overview.found"), cx)
                        .child(kv(&tr("nav.sites"), s.sites.iter().filter(|x| x.domains != ["*"]).count().to_string(), cx))
                        .child(kv(&tr("nav.processes"), s.supervisor.len().to_string(), cx))
                        .child(kv(&tr("nav.databases"), s.databases.iter().map(|d| d.databases.len()).sum::<usize>().to_string(), cx))
                        .child(kv(&tr("overview.cron_jobs"), s.cron.len().to_string(), cx))
                        .child(kv(&tr("overview.certificates"), s.certificates.len().to_string(), cx)),
                ),
        )
}

fn db_slug(k: DatabaseKind) -> &'static str {
    match k {
        DatabaseKind::MySql => "mysql",
        DatabaseKind::MariaDb => "mariadb",
        DatabaseKind::Postgres => "postgresql",
        DatabaseKind::Redis => "redis",
        DatabaseKind::MongoDb => "mongodb",
    }
}

fn db_name(k: DatabaseKind) -> &'static str {
    match k {
        DatabaseKind::MySql => "MySQL",
        DatabaseKind::MariaDb => "MariaDB",
        DatabaseKind::Postgres => "PostgreSQL",
        DatabaseKind::Redis => "Redis",
        DatabaseKind::MongoDb => "MongoDB",
    }
}

// ---- sites -----------------------------------------------------------------

fn site_title(site: &Site) -> String {
    if site.domains == ["*"] { tr("sites.any_host") } else { site.domains.first().cloned().unwrap_or_default() }
}

fn framework_label(site: &Site) -> Option<String> {
    let name = match site.framework {
        Framework::Laravel => "Laravel",
        Framework::WordPress => "WordPress",
        Framework::Node => "Node.js",
        Framework::Static => "Static",
        Framework::Unknown => return None,
    };
    Some(match &site.framework_version {
        Some(v) => format!("{name} {v}"),
        None => name.to_string(),
    })
}

fn sites(app: &SesameApp, server: sesame_core::ServerId, s: &ServerSnapshot, cx: &mut Context<SesameApp>) -> Div {
    let g = Glass::get(cx).clone();
    let q = app.site_search.read(cx).value().trim().to_lowercase();
    let kinds: Vec<Framework> = {
        let mut k: Vec<Framework> = s.sites.iter().map(|x| x.framework).filter(|f| *f != Framework::Unknown).collect();
        k.dedup();
        k
    };

    let mut chips = h_flex().gap_1();
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
    chips = chips.child(
        chip("chip-all".into(), tr("sites.all"), app.site_kind.is_none())
            .on_click(cx.listener(|this, _, _, cx| {
                this.site_kind = None;
                cx.notify();
            })),
    );
    for k in kinds {
        let label = framework_label(&Site { framework: k, ..Default::default() }).unwrap_or_default();
        chips = chips.child(
            chip(format!("chip-{label}"), label, app.site_kind == Some(k)).on_click(cx.listener(move |this, _, _, cx| {
                this.site_kind = Some(k);
                cx.notify();
            })),
        );
    }

    let toolbar = h_flex()
        .gap_3()
        .child(div().w(px(380.)).child(Input::new(&app.site_search).prefix(icon(IconName::Search, g.faint)).cleanable(true)))
        .child(chips);

    let visible: Vec<(usize, &Site)> = s
        .sites
        .iter()
        .enumerate()
        .filter(|(_, site)| app.site_kind.is_none_or(|k| site.framework == k))
        .filter(|(_, site)| {
            q.is_empty()
                || site.domains.iter().any(|d| d.to_lowercase().contains(&q))
                || site.app_path.as_deref().is_some_and(|p| p.to_lowercase().contains(&q))
                || framework_label(site).is_some_and(|f| f.to_lowercase().contains(&q))
        })
        .collect();

    let mut grid = h_flex().flex_wrap().gap_3().items_start();
    for (i, site) in &visible {
        grid = grid.child(site_card(server, *i, site, s, cx));
    }
    let body = if s.sites.is_empty() {
        empty(tr("sites.none"), cx)
    } else if visible.is_empty() {
        empty(trp("sites.no_match", &[("q", &q)]), cx)
    } else {
        grid
    };
    v_flex().gap_4().child(toolbar).child(body)
}

fn site_card(server: sesame_core::ServerId, i: usize, site: &Site, s: &ServerSnapshot, cx: &mut Context<SesameApp>) -> impl IntoElement {
    let g = Glass::get(cx).clone();
    let title = site_title(site);
    let procs: Vec<&SupervisorProgram> = s.supervisor.iter().filter(|p| site.processes.contains(&p.name)).collect();
    let all_running = procs.iter().all(|p| p.state == "RUNNING");
    let status = if all_running { g.ok } else { g.bad };

    let mut tags = h_flex().gap_1().flex_wrap();
    if let Some(f) = framework_label(site) {
        tags = tags.child(
            h_flex()
                .gap_1()
                .px_2()
                .py_0p5()
                .rounded_md()
                .bg(g.panel_hover)
                .text_xs()
                .children(framework_slug(site.framework).and_then(|s| brand(s, px(12.))))
                .child(f),
        );
    }
    if procs.iter().any(|p| p.role == ProcessRole::Octane) {
        tags = tags.child(pill("Octane", g.accent));
    } else if site.php {
        tags = tags.child(pill("PHP", g.info));
    }
    if site.tls.is_some() {
        tags = tags.child(h_flex().gap_1().child(icon(IconName::Lock, g.ok).size_3()).child(div().text_xs().text_color(g.ok).child("TLS")));
    }

    let db = site.env.as_ref().and_then(|e| e.db_database.clone());
    v_flex()
        .id(SharedString::from(format!("site-{i}")))
        .w(px(318.))
        .p_4()
        .gap_3()
        .rounded_lg()
        .border_1()
        .border_color(g.line)
        .bg(g.panel)
        .cursor_pointer()
        .hover(|d| d.bg(g.panel_hover).border_color(g.accent.opacity(0.5)))
        .on_click(cx.listener(move |this, _, _, cx| this.navigate(Page::Server(server, Section::Site(i)), cx)))
        .child(
            h_flex()
                .gap_3()
                .child(avatar(&title, cx))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().font_weight(FontWeight::SEMIBOLD).overflow_hidden().child(title))
                        .child(div().text_xs().text_color(g.faint).overflow_hidden().child(
                            match site.domains.len() {
                                0 | 1 => site.app_path.clone().unwrap_or_default(),
                                n => format!("+{} · {}", n - 1, site.domains[1..].join(", ")),
                            },
                        )),
                )
                .child(dot(status)),
        )
        .child(tags)
        .child(
            h_flex()
                .gap_3()
                .text_xs()
                .text_color(g.dim)
                .when_some(db, |d, db| d.child(h_flex().gap_1().child(icon(IconName::Database, g.faint).size_3()).child(db)))
                .when(!procs.is_empty(), |d| {
                    d.child(
                        h_flex()
                            .gap_1()
                            .child(icon(IconName::Activity, g.faint).size_3())
                            .child(format!("{}", procs.len()))
                            .children(procs.iter().map(|p| dot(state_color(&p.state, cx)).size(px(5.)))),
                    )
                }),
        )
}

fn site_detail(server: sesame_core::ServerId, s: &ServerSnapshot, i: usize, cx: &mut Context<SesameApp>) -> Div {
    let g = Glass::get(cx).clone();
    let Some(site) = s.sites.get(i) else { return empty(tr("sites.none"), cx) };
    let title = site_title(site);
    let url = site.domains.first().filter(|d| *d != "*").map(|d| format!("https://{d}"));

    let back = h_flex()
        .id("back")
        .gap_1()
        .cursor_pointer()
        .text_color(g.dim)
        .hover(|d| d.text_color(g.text))
        .on_click(cx.listener(move |this, _, _, cx| this.navigate(Page::Server(server, Section::Sites), cx)))
        .child("←")
        .child(tr("sites.back"));

    let head = h_flex()
        .gap_3()
        .child(avatar(&title, cx).size(px(44.)).text_lg())
        .child(
            v_flex()
                .flex_1()
                .child(div().text_xl().font_weight(FontWeight::BOLD).child(title.clone()))
                .child(div().text_color(g.dim).child(site.domains.join(" · "))),
        )
        .when_some(url, |d, url| {
            d.child(
                Button::new("open")
                    .icon(IconName::ExternalLink)
                    .label(tr("sites.open"))
                    .on_click(move |_, _, cx| cx.open_url(&url)),
            )
        })
        .child(Button::new("maint").icon(IconName::Construction).label(tr("sites.maintenance")).disabled(true))
        .child(Button::new("disable").icon(IconName::PowerOff).label(tr("sites.disable")).disabled(true));

    let fw = framework_label(site).unwrap_or_else(|| "—".into());
    let mut info = card(&tr("nav.sites"), cx)
        .child(kv(&tr("sites.framework"), fw, cx))
        .child(kv(&tr("sites.directory"), site.app_path.clone().unwrap_or_else(|| "—".into()), cx))
        .child(kv(
            &tr("sites.served_by"),
            format!(
                "{}{}{}",
                site.served_by.map(|k| format!("{k:?}")).unwrap_or_default(),
                if site.php { " + PHP" } else { "" },
                if site.upstreams.is_empty() { String::new() } else { format!(" → {}", site.upstreams.join(", ")) }
            ),
            cx,
        ))
        .child(kv(&tr("sites.tls"), site.tls.clone().unwrap_or_else(|| tr("sites.tls_none")), cx));
    if let Some(env) = &site.env {
        let db = match (&env.db_connection, &env.db_database) {
            (Some(c), Some(d)) => format!("{c} · {d} @ {}", env.db_host.clone().unwrap_or_default()),
            (Some(c), None) => c.clone(),
            _ => "—".into(),
        };
        info = info
            .child(kv(&tr("sites.environment"), env.app_env.clone().unwrap_or_else(|| "—".into()), cx))
            .child(kv(&tr("sites.database"), db, cx))
            .child(kv(
                &tr("sites.drivers"),
                trp(
                    "sites.drivers_value",
                    &[
                        ("cache", env.cache_store.as_deref().unwrap_or("—")),
                        ("queue", env.queue_connection.as_deref().unwrap_or("—")),
                        ("session", env.session_driver.as_deref().unwrap_or("—")),
                    ],
                ),
                cx,
            ))
            .child(kv(
                &tr("sites.mail"),
                match (&env.mail_mailer, &env.mail_host) {
                    (Some(m), Some(h)) => format!("{m} via {h}"),
                    (Some(m), None) => m.clone(),
                    _ => "—".into(),
                },
                cx,
            ));
    }

    let procs: Vec<&SupervisorProgram> = s.supervisor.iter().filter(|p| site.processes.contains(&p.name)).collect();
    let mut proc_card = card(&tr("sites.processes"), cx);
    for p in &procs {
        proc_card = proc_card.child(process_row(p, cx));
    }
    let users: Vec<String> = {
        let mut u: Vec<String> = procs.iter().filter_map(|p| p.user.clone()).collect();
        u.sort();
        u.dedup();
        u
    };
    let isolation = card(&tr("sites.isolation"), cx).child(kv(
        "Linux",
        if users.is_empty() { "—".into() } else { users.join(", ") },
        cx,
    ));

    v_flex()
        .gap_4()
        .child(back)
        .child(head)
        .child(div().text_xs().text_color(g.faint).child(tr("sites.actions_soon")))
        .child(info)
        .when(!procs.is_empty(), |d| d.child(proc_card))
        .child(isolation)
}

// ---- processes ---------------------------------------------------------------

fn process_row(p: &SupervisorProgram, cx: &App) -> Div {
    let g = Glass::get(cx);
    let color = state_color(&p.state, cx);
    h_flex()
        .gap_3()
        .min_h(px(28.))
        .child(dot(color))
        .child(div().w(px(80.)).child(pill(p.state.clone(), color)))
        .child(div().flex_1().min_w_0().font_weight(FontWeight::MEDIUM).overflow_hidden().child(p.name.clone()))
        .child(div().w(px(80.)).text_color(g.dim).child(format!("{:?}", p.role).to_lowercase()))
        .child(div().w(px(80.)).text_color(g.dim).child(p.user.clone().unwrap_or_default()))
        .child(div().w(px(140.)).text_color(g.faint).child(p.uptime.clone().map(|u| trp("proc.up", &[("time", &u)])).unwrap_or_default()))
}

fn processes(s: &ServerSnapshot, cx: &App) -> Div {
    let g = Glass::get(cx);
    let mut sup = card(&tr("proc.supervisor"), cx);
    if s.supervisor.is_empty() {
        sup = sup.child(div().text_color(g.dim).child(tr("proc.none")));
    }
    for p in &s.supervisor {
        sup = sup.child(process_row(p, cx));
    }
    let mut sys = card(&tr("proc.system"), cx);
    for svc in s.services.iter().filter(|x| x.active != "inactive") {
        let color = state_color(&svc.sub, cx);
        sys = sys.child(
            h_flex()
                .gap_3()
                .min_h(px(26.))
                .child(dot(color))
                .child(div().w(px(80.)).child(pill(svc.sub.clone(), color)))
                .child(div().w(px(240.)).child(svc.unit.trim_end_matches(".service").to_string()))
                .child(div().flex_1().text_color(g.dim).child(svc.description.clone())),
        );
    }
    v_flex().gap_4().child(sup).child(sys)
}

// ---- services catalog ----------------------------------------------------------

struct CatalogItem {
    name: &'static str,
    slug: &'static str,
    desc: &'static str,
    detect: fn(&ServerSnapshot) -> Option<String>,
    conflict: fn(&ServerSnapshot) -> Option<&'static str>,
    per_site: bool,
}

fn engine(s: &ServerSnapshot, kind: DatabaseKind) -> Option<String> {
    s.databases.iter().find(|d| d.kind == kind).map(|d| d.version.clone().unwrap_or_default())
}

fn unit_or_port(s: &ServerSnapshot, units: &[&str], procs: &[&str]) -> Option<String> {
    let unit = s.services.iter().find(|u| units.iter().any(|n| u.unit.starts_with(n)));
    let port = s.ports.iter().find(|p| procs.contains(&p.process.as_str()));
    match (unit, port) {
        (Some(u), _) => Some(u.sub.clone()),
        (None, Some(p)) => Some(format!(":{}", p.port)),
        _ => None,
    }
}

const CATALOG: &[CatalogItem] = &[
    CatalogItem { name: "Redis", slug: "redis", desc: "svc.redis", detect: |s| engine(s, DatabaseKind::Redis), conflict: |_| None, per_site: false },
    CatalogItem { name: "PostgreSQL", slug: "postgresql", desc: "svc.postgres", detect: |s| engine(s, DatabaseKind::Postgres), conflict: |_| None, per_site: false },
    CatalogItem {
        name: "MariaDB",
        slug: "mariadb",
        desc: "svc.mariadb",
        detect: |s| engine(s, DatabaseKind::MariaDb),
        conflict: |s| s.databases.iter().any(|d| d.kind == DatabaseKind::MySql).then_some("MySQL"),
        per_site: false,
    },
    CatalogItem { name: "MongoDB", slug: "mongodb", desc: "svc.mongodb", detect: |s| engine(s, DatabaseKind::MongoDb), conflict: |_| None, per_site: false },
    CatalogItem {
        name: "Elasticsearch",
        slug: "elasticsearch",
        desc: "svc.elasticsearch",
        detect: |s| unit_or_port(s, &["elasticsearch"], &["java"]).filter(|_| s.ports.iter().any(|p| p.port == 9200)),
        conflict: |_| None,
        per_site: false,
    },
    CatalogItem {
        name: "S3 · RustFS / MinIO",
        slug: "minio",
        desc: "svc.s3",
        detect: |s| unit_or_port(s, &["rustfs", "minio"], &["rustfs", "minio"]),
        conflict: |_| None,
        per_site: false,
    },
    CatalogItem {
        name: "Reverb",
        slug: "laravel",
        desc: "svc.reverb",
        detect: |s| {
            let n = s.supervisor.iter().filter(|p| p.role == ProcessRole::Reverb).count();
            (n > 0).then(|| n.to_string())
        },
        conflict: |_| None,
        per_site: true,
    },
];

fn services(s: &ServerSnapshot, cx: &App) -> Div {
    let g = Glass::get(cx);
    let mut grid = div().grid().grid_cols(2).gap_3();
    for (i, item) in CATALOG.iter().enumerate() {
        let found = (item.detect)(s);
        let conflict = (item.conflict)(s);
        let status = match (&found, conflict) {
            (Some(v), _) if !v.is_empty() => pill(format!("{} · {v}", tr("svc.installed")), g.ok),
            (Some(_), _) => pill(tr("svc.installed"), g.ok),
            (None, Some(other)) => pill(trp("svc.conflict", &[("other", other)]), g.warn),
            (None, None) => pill(tr("svc.not_installed"), g.faint),
        };
        let action = if found.is_some() && !item.per_site {
            Button::new(SharedString::from(format!("svc-{i}"))).ghost().label(tr("svc.uninstall"))
        } else {
            Button::new(SharedString::from(format!("svc-{i}")))
                .primary()
                .icon(IconName::Download)
                .label(if item.per_site { tr("svc.per_site") } else { tr("svc.install") })
        };
        grid = grid.child(
            panel(cx)
                .p_4()
                .gap_3()
                .child(
                    h_flex()
                        .gap_3()
                        .child(
                            div()
                                .size(px(36.))
                                .rounded_lg()
                                .bg(g.panel_hover)
                                .flex()
                                .items_center()
                                .justify_center()
                                .children(brand(item.slug, px(20.))),
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .child(div().font_weight(FontWeight::SEMIBOLD).child(item.name))
                                .child(div().text_xs().text_color(g.dim).child(tr(item.desc))),
                        ),
                )
                .child(h_flex().justify_between().child(status).child(action.disabled(true))),
        );
    }
    v_flex()
        .gap_4()
        .child(
            v_flex()
                .gap_1()
                .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(tr("svc.title")))
                .child(div().text_color(g.dim).child(tr("svc.lead"))),
        )
        .child(grid)
        .child(div().text_xs().text_color(g.faint).child(tr("svc.soon")))
}

// ---- databases -----------------------------------------------------------------

fn databases(s: &ServerSnapshot, cx: &App) -> Div {
    let g = Glass::get(cx);
    let mut col = v_flex().gap_4();
    if s.databases.is_empty() {
        return col.child(empty(tr("db.none"), cx));
    }
    for d in &s.databases {
        let used_by: Vec<String> = s
            .sites
            .iter()
            .filter(|site| {
                site.env.as_ref().is_some_and(|e| {
                    e.db_database.as_ref().is_some_and(|db| d.databases.contains(db))
                        || (d.kind == DatabaseKind::Redis && e.redis_host.is_some())
                })
            })
            .map(site_title)
            .collect();
        let mut dbs = h_flex().gap_1().flex_wrap();
        if d.databases.is_empty() {
            dbs = dbs.child(div().text_color(g.faint).child(tr("db.needs_creds")));
        }
        for name in &d.databases {
            dbs = dbs.child(
                h_flex()
                    .gap_1()
                    .px_2()
                    .py_0p5()
                    .rounded_md()
                    .bg(g.panel_hover)
                    .child(icon(IconName::Database, g.faint).size_3())
                    .child(mono(name.clone())),
            );
        }
        col = col.child(
            panel(cx)
                .p_4()
                .gap_3()
                .child(
                    h_flex()
                        .gap_3()
                        .children(brand(db_slug(d.kind), px(22.)))
                        .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(db_name(d.kind)))
                        .child(div().text_color(g.dim).child(d.version.clone().unwrap_or_default())),
                )
                .child(kv(&tr("db.listens"), d.listen.clone().unwrap_or_else(|| "—".into()), cx))
                .child(h_flex().gap_4().child(div().w(px(140.)).text_color(g.dim).child(tr("db.databases"))).child(dbs))
                .when(!used_by.is_empty(), |c| c.child(kv(&tr("db.used_by"), used_by.join(", "), cx))),
        );
    }
    col
}

// ---- mail & cron -----------------------------------------------------------------

fn mail_cron(s: &ServerSnapshot, cx: &App) -> Div {
    let g = Glass::get(cx);
    let mut mail = card(&tr("mail.title"), cx);
    if s.mail.servers.is_empty() {
        mail = mail.child(kv(&tr("mail.server"), tr("mail.none_local"), cx));
        if !s.mail.external.is_empty() {
            mail = mail.child(kv(&tr("mail.apps_send"), s.mail.external.join(", "), cx));
        }
    } else {
        mail = mail
            .child(kv(&tr("mail.running"), s.mail.servers.join(", "), cx))
            .child(kv(&tr("mail.hostname"), s.mail.hostname.clone().unwrap_or_else(|| "—".into()), cx))
            .child(kv(&tr("mail.domains"), if s.mail.domains.is_empty() { "—".into() } else { s.mail.domains.join(", ") }, cx));
    }
    let mut cron = card(&tr("cron.title"), cx);
    if s.cron.is_empty() {
        cron = cron.child(div().text_color(g.dim).child(tr("cron.none")));
    }
    for c in &s.cron {
        cron = cron.child(
            h_flex()
                .gap_3()
                .min_h(px(26.))
                .child(div().w(px(110.)).child(mono(c.schedule.clone()).text_color(g.accent)))
                .child(div().w(px(70.)).text_color(g.dim).child(c.user.clone()))
                .child(div().flex_1().min_w_0().overflow_hidden().child(mono(c.command.clone()))),
        );
    }
    v_flex().gap_4().child(mail).child(cron)
}

// ---- security ----------------------------------------------------------------------

fn security(s: &ServerSnapshot, cx: &App) -> Div {
    let g = Glass::get(cx);
    let fw = card(&tr("sec.firewall"), cx)
        .child(kv("ufw", s.firewall.ufw.clone().unwrap_or_else(|| tr("sec.not_installed")), cx))
        .child(kv(
            "fail2ban",
            if s.firewall.fail2ban_jails.is_empty() { tr("sec.no_jails") } else { s.firewall.fail2ban_jails.join(", ") },
            cx,
        ));
    let mut ports = card(&tr("sec.ports"), cx);
    for p in &s.ports {
        ports = ports.child(
            h_flex()
                .gap_3()
                .min_h(px(26.))
                .child(div().w(px(60.)).child(mono(p.port.to_string())))
                .child(div().w(px(80.)).child(if p.public { pill(tr("sec.public"), g.warn) } else { pill(tr("sec.local"), g.faint) }))
                .child(div().w(px(140.)).text_color(g.dim).child(mono(p.address.clone())))
                .child(div().flex_1().child(p.process.clone())),
        );
    }
    let mut certs = card(&tr("sec.certs"), cx);
    if s.certificates.is_empty() {
        certs = certs.child(div().text_color(g.dim).child(tr("sec.no_certs")));
    }
    for c in &s.certificates {
        certs = certs.child(
            h_flex()
                .gap_3()
                .child(icon(IconName::Lock, g.ok))
                .child(
                    v_flex()
                        .child(mono(c.path.clone()))
                        .child(div().text_xs().text_color(g.dim).child(format!(
                            "{} · {}",
                            c.names.join(", "),
                            trp("sec.expires", &[("date", &c.not_after.as_deref().map(short_time).unwrap_or_else(|| "?".into()))])
                        ))),
                ),
        );
    }
    v_flex().gap_4().child(fw).child(ports).child(certs)
}

// ---- add server ----------------------------------------------------------------------

pub fn add_server(app: &SesameApp, cx: &mut Context<SesameApp>) -> impl IntoElement {
    let g = Glass::get(cx).clone();
    let field = |label: String, hint: String, input: &Entity<gpui_kit::component::input::InputState>| {
        v_flex()
            .gap_1()
            .child(div().font_weight(FontWeight::MEDIUM).child(label))
            .child(Input::new(input))
            .child(div().text_xs().text_color(g.faint).child(hint))
    };
    let form = v_flex()
        .w(px(560.))
        .gap_5()
        .child(
            v_flex()
                .items_center()
                .gap_2()
                .child(img(ImageSource::Resource(Resource::Embedded("logo.svg".into()))).size(px(56.)))
                .child(div().text_xl().font_weight(FontWeight::BOLD).child(tr("add.title")))
                .child(div().text_center().text_color(g.dim).child(tr("add.lead"))),
        )
        .child(
            panel(cx)
                .p_5()
                .gap_4()
                .child(field(tr("add.address"), tr("add.address_hint"), &app.form.target))
                .child(field(tr("add.name"), tr("add.name_hint"), &app.form.name))
                .child(field(tr("add.key"), tr("add.optional"), &app.form.key))
                .child(field(tr("add.password"), tr("add.password_hint"), &app.form.password))
                .when_some(app.form.error.clone(), |d, e| d.child(div().text_color(g.bad).child(e)))
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("connect")
                                .primary()
                                .icon(IconName::DoorOpen)
                                .label(tr("add.submit"))
                                .on_click(cx.listener(|this, _, _, cx| this.submit_add(cx))),
                        )
                        .when(!app.servers.is_empty(), |d| {
                            d.child(Button::new("cancel").ghost().label(tr("add.cancel")).on_click(cx.listener(|this, _, _, cx| {
                                let first = this.servers.first().map(|s| s.id);
                                if let Some(id) = first {
                                    this.navigate(Page::Server(id, Section::Overview), cx);
                                }
                            })))
                        }),
                ),
        );
    div().id("add").size_full().overflow_y_scroll().flex().justify_center().pt_10().child(form)
}

// ---- settings ----------------------------------------------------------------------

pub fn settings(app: &SesameApp, _window: &mut Window, cx: &mut Context<SesameApp>) -> impl IntoElement {
    let g = Glass::get(cx).clone();
    let this = cx.entity().downgrade();
    let theme_pick = {
        let this = this.clone();
        move |pref: ThemePref, window: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| app.set_theme(pref, window, cx)).ok();
        }
    };
    let lang_pick = {
        let this = this.clone();
        move |pref: LangPref, window: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| app.set_language(pref, window, cx)).ok();
        }
    };
    let channel_pick = {
        let this = this.clone();
        move |ch: Channel, _: &mut Window, cx: &mut App| {
            this.update(cx, |app, cx| {
                let _ = app.store.set_setting("update.channel", ch.as_str());
                app.check_updates(cx);
            })
            .ok();
        }
    };
    let row = |label: String, control: AnyElement| {
        h_flex().gap_4().min_h(px(36.)).child(div().w(px(180.)).text_color(g.dim).child(label)).child(control)
    };

    let update_status = if app.update.checking {
        tr("update.checking")
    } else if let Some(e) = &app.update.error {
        trp("update.failed", &[("error", e)])
    } else if let Some(info) = &app.update.info {
        trp("update.available", &[("version", &info.latest.version.to_string())])
    } else if app.update.checked {
        trp("update.up_to_date", &[("version", env!("CARGO_PKG_VERSION"))])
    } else {
        String::new()
    };

    let body = v_flex()
        .gap_4()
        .max_w(px(760.))
        .child(
            card(&tr("settings.appearance"), cx)
                .child(row(
                    tr("settings.theme"),
                    segmented(
                        "theme",
                        &[
                            (ThemePref::System, tr("settings.system")),
                            (ThemePref::Light, tr("settings.light")),
                            (ThemePref::Dark, tr("settings.dark")),
                        ],
                        app.theme_pref,
                        theme_pick,
                        cx,
                    )
                    .into_any_element(),
                ))
                .child(row(
                    tr("settings.language"),
                    segmented(
                        "lang",
                        &[(LangPref::System, tr("settings.system")), (LangPref::En, "English".into()), (LangPref::Ru, "Русский".into())],
                        app.lang_pref,
                        lang_pick,
                        cx,
                    )
                    .into_any_element(),
                )),
        )
        .child(
            card(&tr("settings.updates"), cx)
                .child(row(
                    tr("settings.channel"),
                    segmented(
                        "channel",
                        &[(Channel::Stable, tr("settings.stable")), (Channel::Beta, tr("settings.beta"))],
                        app.channel(),
                        channel_pick,
                        cx,
                    )
                    .into_any_element(),
                ))
                .child(row(
                    String::new(),
                    Switch::new("auto-update")
                        .checked(app.auto_update())
                        .label(tr("settings.auto_update"))
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            let _ = this.store.set_setting("update.auto", if *checked { "true" } else { "false" });
                            cx.notify();
                        }))
                        .into_any_element(),
                ))
                .child(row(
                    tr("settings.version"),
                    h_flex()
                        .gap_3()
                        .child(mono(env!("CARGO_PKG_VERSION")))
                        .child(
                            Button::new("check-now")
                                .ghost()
                                .loading(app.update.checking)
                                .icon(IconName::RefreshCw)
                                .label(tr("settings.check_now"))
                                .on_click(cx.listener(|this, _, _, cx| this.check_updates(cx))),
                        )
                        .child(div().text_color(g.dim).child(update_status))
                        .into_any_element(),
                )),
        )
        .child(
            card(&tr("settings.about"), cx)
                .child(h_flex().gap_3().child(img(ImageSource::Resource(Resource::Embedded("logo.svg".into()))).size(px(32.))).child(
                    v_flex()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(format!("Sesame {}", env!("CARGO_PKG_VERSION"))))
                        .child(div().text_xs().text_color(g.dim).child(tr("settings.license"))),
                ))
                .child(
                    h_flex().child(
                        Button::new("github")
                            .ghost()
                            .icon(IconName::ExternalLink)
                            .label("github.com/rasul-chopurov/sesame")
                            .on_click(|_, _, cx| cx.open_url("https://github.com/rasul-chopurov/sesame")),
                    ),
                ),
        );
    page(cx)
        .child(div().px_5().py_3().border_b_1().border_color(g.line).child(div().text_lg().font_weight(FontWeight::BOLD).child(tr("nav.settings"))))
        .child(scroll("settings", body))
}

// ---- updates -------------------------------------------------------------------------

pub fn updates(app: &SesameApp, cx: &mut Context<SesameApp>) -> impl IntoElement {
    let g = Glass::get(cx).clone();
    let mut body = v_flex().gap_4().max_w(px(820.));
    match &app.update.info {
        None => {
            body = body.child(empty(
                if app.update.checking { tr("update.checking") } else { trp("update.up_to_date", &[("version", env!("CARGO_PKG_VERSION"))]) },
                cx,
            ));
        }
        Some(info) => {
            let dev = sesame_update::installed_location().is_none();
            let mut actions = h_flex().gap_2();
            if let Some(_) = &app.update.ready {
                actions = actions.child(
                    Button::new("restart").primary().icon(IconName::Rocket).label(tr("update.restart"))
                        .on_click(cx.listener(|this, _, _, cx| this.restart_into_update(cx))),
                );
            } else {
                actions = actions
                    .child(
                        Button::new("install")
                            .primary()
                            .icon(IconName::Download)
                            .loading(app.update.downloading)
                            .disabled(dev || info.latest.asset.is_none())
                            .label(if app.update.downloading { tr("update.downloading") } else { tr("update.install") })
                            .on_click(cx.listener(|this, _, _, cx| this.install_update(cx))),
                    )
                    .child(Button::new("skip").ghost().label(tr("update.skip")).on_click(cx.listener(|this, _, _, cx| this.skip_update(cx))));
            }
            body = body.child(
                panel(cx)
                    .p_5()
                    .gap_3()
                    .child(
                        h_flex()
                            .gap_3()
                            .child(icon(IconName::Sparkles, g.accent))
                            .child(div().text_xl().font_weight(FontWeight::BOLD).child(format!("Sesame {}", info.latest.version)))
                            .when(info.latest.prerelease, |d| d.child(pill("pre-release", g.warn))),
                    )
                    .child(actions)
                    .when(dev, |d| d.child(div().text_xs().text_color(g.faint).child(tr("update.dev_build"))))
                    .when_some(app.update.error.clone(), |d, e| d.child(div().text_color(g.bad).child(e))),
            );
            for (i, r) in info.changelog.iter().enumerate() {
                body = body.child(
                    card(&format!("{}  ·  {}", r.version, r.published_at.as_deref().map(short_time).unwrap_or_default()), cx)
                        .child(TextView::markdown(SharedString::from(format!("notes-{i}")), r.notes.clone())),
                );
            }
        }
    }
    page(cx)
        .child(div().px_5().py_3().border_b_1().border_color(g.line).child(div().text_lg().font_weight(FontWeight::BOLD).child(tr("nav.updates"))))
        .child(scroll("updates", body))
}
