//! Renders every screen offscreen (Metal) from a recorded snapshot in light
//! and dark, English and Russian, and saves PNGs to `$SESAME_SHOTS_DIR`
//! (default `target/screenshots`). Any panic in a view fails the run.

use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Bounds, HeadlessAppContext, WindowBounds, WindowOptions, px, size};
use sesame_app::ui::{Page, Section, SesameApp};
use sesame_core::{ServerConfig, SqliteStore, Store};

fn main() {
    // Never touch the user's real data directory from tests.
    unsafe { std::env::set_var("SESAME_TEST_ISOLATION", "1") };
    let dir = std::env::var("SESAME_SHOTS_DIR")
        .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/screenshots").to_string());
    std::fs::create_dir_all(&dir).unwrap();
    let only = std::env::var("SESAME_SHOTS_ONLY").ok();

    let mut snap = sesame_discovery::from_probe(include_str!("../../sesame-discovery/fixtures/frankenphp-octane.probe"));
    sesame_discovery::apply_details(&mut snap, include_str!("../../sesame-discovery/fixtures/frankenphp-octane.details"));
    sesame_discovery::link(&mut snap);
    // Optionally render a real server's snapshot (`sesame discover <server> > snap.json`).
    if let Ok(path) = std::env::var("SESAME_SHOTS_SNAPSHOT") {
        snap = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    }

    for (theme, lang) in [("light", "en"), ("dark", "en"), ("light", "ru"), ("dark", "ru"), ("light", "az"), ("dark", "az")] {
        let store = Arc::new(SqliteStore::in_memory().unwrap());
        let server = ServerConfig::new("app-1", "203.0.113.10", "root");
        store.save_server(&server).unwrap();
        store.save_server(&ServerConfig::new("staging", "198.51.100.7", "deploy")).unwrap();
        store.save_snapshot(server.id, &snap).unwrap();
        store.set_setting("theme", theme).unwrap();
        store.set_setting("language", lang).unwrap();

        let mut cx = HeadlessAppContext::with_platform(
            gpui_kit::platform::current_platform(true).text_system(),
            Arc::new(sesame_app::assets::AppAssets),
            gpui_kit::platform::current_headless_renderer,
        );
        cx.update(gpui_kit::init);
        let (handle, view) = cx
            .update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin: Default::default(),
                            size: size(px(1320.), px(840.)),
                        })),
                        focus: false,
                        show: false,
                        ..Default::default()
                    },
                    cx,
                    |window, cx| cx.new(|cx| SesameApp::new(store, false, window, cx)),
                )
            })
            .unwrap();

        let id = server.id;
        cx.update_window(handle, |_, _, cx| view.update(cx, |v, cx| v.demo_integrations("203.0.113.10", cx))).unwrap();
        let mut pages: Vec<(&str, Page)> = vec![
            ("overview", Page::Server(id, Section::Overview)),
            ("sites", Page::Server(id, Section::Sites)),
            ("site", Page::Server(id, Section::Site(0))),
            ("new-site", Page::Server(id, Section::NewSite)),
            ("files", Page::Server(id, Section::Files(0))),
            ("processes", Page::Server(id, Section::Processes)),
            ("services", Page::Server(id, Section::Services)),
            ("databases", Page::Server(id, Section::Databases)),
            ("database-engine", Page::Server(id, Section::Databases)),
            ("mail-cron", Page::Server(id, Section::MailCron)),
            ("security", Page::Server(id, Section::Security)),
            ("add-server", Page::AddServer),
            ("settings", Page::Settings),
            ("updates", Page::Updates),
            ("vault", Page::Vault),
            ("cloudflare", Page::Cloudflare),
            ("cloudflare-zone", Page::Cloudflare),
        ];
        if let Some(only) = &only {
            pages.retain(|(name, _)| only.split(',').any(|o| o == *name));
        }
        let want = |name: &str| only.as_ref().is_none_or(|o| o.split(',').any(|x| x == name));
        for (name, page) in pages {
            cx.update_window(handle, |_, window, cx| {
                view.update(cx, |v, cx| {
                    v.navigate(page, cx);
                    if name == "files" {
                        v.demo_files(id, 0, cx);
                    } else if name == "database-engine" {
                        v.demo_open_engine(sesame_core::snapshot::DatabaseKind::MySql, cx);
                    } else if name == "cloudflare-zone" {
                        v.demo_open_zone("z1", cx);
                    } else if name == "cloudflare" {
                        v.demo_integrations("203.0.113.10", cx);
                    }
                });
                window.render_frame(cx);
            })
            .unwrap();
            // Images load asynchronously; let them land, then paint again.
            cx.run_until_parked();
            cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
            let image = cx.capture_screenshot(handle).expect("offscreen rendering");
            let path = format!("{dir}/{name}-{theme}-{lang}.png");
            image.save(&path).unwrap();
            println!("saved {path}");
        }

        // Confirmation dialog for a destructive plan, then a finished job with credentials.
        let shots: Vec<(&str, Box<dyn Fn(&mut SesameApp, &mut gpui_kit::Window, &mut gpui_kit::Context<SesameApp>)>)> = vec![
            (
                "confirm-drop",
                Box::new(|v, w, cx| {
                    v.navigate(Page::Server(id, Section::Databases), cx);
                    let plan = sesame_ops::actions::db_drop(sesame_core::snapshot::DatabaseKind::MySql, "shop", None, None).unwrap();
                    v.demo_confirm(plan, w, cx);
                }),
            ),
            (
                "job-done",
                Box::new(|v, _, cx| {
                    v.navigate(Page::Server(id, Section::Services), cx);
                    let plan = sesame_provision::install(sesame_provision::Service::Postgres, &Default::default()).unwrap();
                    let lines = ["Reading package lists...", "Setting up postgresql-18 (18.1-1.pgdg24.04+1) ...", "Created symlink /etc/systemd/system/multi-user.target.wants/postgresql.service", "PostgreSQL 18.1 on aarch64-unknown-linux-gnu"];
                    let mut plan = plan;
                    plan.reveal = vec![("DB_PASSWORD".into(), "k7Qm2vXc9RtA4pLs8NwE3hJy6UbZ5dFg".into())];
                    v.demo_job(&plan, lines.iter().map(|s| s.to_string()).collect(), cx);
                }),
            ),
        ];
        for (name, apply) in shots.iter().filter(|(n, _)| want(n)) {
            cx.update_window(handle, |_, window, cx| {
                view.update(cx, |v, cx| apply(v, window, cx));
                window.render_frame(cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
            let image = cx.capture_screenshot(handle).expect("offscreen rendering");
            let path = format!("{dir}/{name}-{theme}-{lang}.png");
            image.save(&path).unwrap();
            println!("saved {path}");
        }
    }
}
