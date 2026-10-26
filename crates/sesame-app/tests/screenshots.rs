//! Renders every screen offscreen (Metal) from a recorded snapshot and saves
//! PNGs to `$SESAME_SHOTS_DIR` (default `target/screenshots`). Also a smoke
//! test: any panic in a view fails the run.

use std::sync::Arc;

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Bounds, HeadlessAppContext, WindowBounds, WindowOptions, px, size};
use sesame_app::ui::{SesameApp, Tab};
use sesame_core::{ServerConfig, SqliteStore, Store};

fn main() {
    let dir = std::env::var("SESAME_SHOTS_DIR").unwrap_or_else(|_| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/screenshots").to_string()
    });
    std::fs::create_dir_all(&dir).unwrap();

    let store = Arc::new(SqliteStore::in_memory().unwrap());
    let server = ServerConfig::new("app-1", "203.0.113.10", "root");
    store.save_server(&server).unwrap();
    store.save_server(&ServerConfig::new("staging", "198.51.100.7", "deploy")).unwrap();
    let mut snap = sesame_discovery::from_probe(include_str!("../../sesame-discovery/fixtures/frankenphp-octane.probe"));
    sesame_discovery::apply_details(&mut snap, include_str!("../../sesame-discovery/fixtures/frankenphp-octane.details"));
    sesame_discovery::link(&mut snap);
    store.save_snapshot(server.id, &snap).unwrap();

    for (mode, suffix) in [(ThemeMode::Light, "light"), (ThemeMode::Dark, "dark")] {
        let mut cx = HeadlessAppContext::with_platform(
            gpui_kit::platform::current_platform(true).text_system(),
            Arc::new(gpui_kit::assets::Assets),
            gpui_kit::platform::current_headless_renderer,
        );
        cx.update(gpui_kit::init);
        cx.update(|cx| Theme::change(mode, None, cx));

        let store = store.clone();
        let (handle, view) = cx
            .update(|cx| {
                gpui_kit::open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin: Default::default(),
                            size: size(px(1280.), px(820.)),
                        })),
                        focus: false,
                        show: false,
                        ..Default::default()
                    },
                    cx,
                    |window, cx| cx.new(|cx| SesameApp::new(store, window, cx)),
                )
            })
            .unwrap();

        let mut shots: Vec<(String, Box<dyn Fn(&mut SesameApp, &mut gpui_kit::Context<SesameApp>)>)> = Tab::ALL
            .iter()
            .map(|&tab| {
                let name = tab.label().to_lowercase().replace(" & ", "-");
                (name, Box::new(move |v: &mut SesameApp, cx: &mut gpui_kit::Context<SesameApp>| v.set_tab(tab, cx)) as Box<_>)
            })
            .collect();
        shots.push(("add-server".into(), Box::new(|v, cx| v.show_add_form(cx))));

        for (name, apply) in &shots {
            cx.update_window(handle, |_, window, cx| {
                view.update(cx, |v, cx| apply(v, cx));
                window.render_frame(cx);
            })
            .unwrap();
            let image = cx.capture_screenshot(handle).expect("offscreen rendering");
            let path = format!("{dir}/{name}-{suffix}.png");
            image.save(&path).unwrap();
            println!("saved {path}");
        }
    }
}
