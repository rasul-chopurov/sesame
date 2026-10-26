//! ⌘K: jump to any server, section or site, or run a common action.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use sesame_core::ServerId;

use super::theme::{Glass, ThemePref};
use super::widgets::*;
use super::{Page, Section, SesameApp};
use crate::i18n::tr;

actions!(sesame, [OpenPalette]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("cmd-k", OpenPalette, None), KeyBinding::new("ctrl-k", OpenPalette, None)]);
}

#[derive(Clone)]
enum Cmd {
    Go(Page),
    Rediscover(ServerId),
    Theme(ThemePref),
}

struct Item {
    icon: IconName,
    label: String,
    hint: String,
    cmd: Cmd,
}

pub struct Palette {
    pub input: Entity<InputState>,
    pub selected: usize,
    _sub: Subscription,
}

impl SesameApp {
    pub(crate) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            self.palette = None;
            return cx.notify();
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(tr("palette.placeholder")));
        let sub = cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| {
            match ev {
                InputEvent::PressEnter { .. } => this.palette_run(window, cx),
                InputEvent::Change => {
                    if let Some(p) = this.palette.as_mut() {
                        p.selected = 0;
                    }
                }
                _ => {}
            }
            cx.notify();
        });
        input.update(cx, |s, cx| s.focus(window, cx));
        self.palette = Some(Palette { input, selected: 0, _sub: sub });
        cx.notify();
    }

    /// Opens the palette with a query, over a clean page (screenshots).
    #[doc(hidden)]
    pub fn demo_palette(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.job = None;
        self.pending = None;
        self.palette = None;
        self.open_palette(window, cx);
        if let Some(p) = &self.palette {
            p.input.update(cx, |s, cx| s.set_value(query.to_string(), window, cx));
        }
    }

    /// Shows an agent's request for approval (screenshots).
    #[doc(hidden)]
    pub fn demo_approval(&mut self, server: ServerId, cx: &mut Context<Self>) {
        self.palette = None;
        self.approvals = vec![sesame_core::Approval {
            id: "demo".into(),
            created_at: String::new(),
            actor: "agent:claude-code".into(),
            server_id: server,
            title: "install Redis".into(),
            details: vec!["listens on 127.0.0.1:6379".into(), "password generated and stored in the vault".into()],
            risk: "change".into(),
            status: "pending".into(),
        }];
        cx.notify();
    }

    fn palette_items(&self, cx: &App) -> Vec<Item> {
        let q = self.palette.as_ref().map(|p| p.input.read(cx).value().trim().to_lowercase()).unwrap_or_default();
        let mut items = Vec::new();
        for s in &self.servers {
            for section in Section::NAV {
                items.push(Item { icon: section.icon(), label: format!("{} › {}", s.name, section.label()), hint: tr("palette.go"), cmd: Cmd::Go(Page::Server(s.id, section)) });
            }
            items.push(Item { icon: IconName::Plus, label: format!("{} › {}", s.name, tr("ns.title")), hint: tr("palette.go"), cmd: Cmd::Go(Page::Server(s.id, Section::NewSite)) });
            items.push(Item { icon: IconName::RefreshCw, label: format!("{} › {}", s.name, tr("server.rediscover")), hint: tr("palette.run"), cmd: Cmd::Rediscover(s.id) });
            if let Some(snap) = self.snapshots.get(&s.id) {
                for (i, site) in snap.sites.iter().enumerate() {
                    let name = site.domains.first().cloned().unwrap_or_default();
                    items.push(Item { icon: IconName::Globe, label: name.clone(), hint: s.name.clone(), cmd: Cmd::Go(Page::Server(s.id, Section::Site(i))) });
                    items.push(Item { icon: IconName::Folder, label: format!("{name} › {}", tr("files.title")), hint: s.name.clone(), cmd: Cmd::Go(Page::Server(s.id, Section::Files(i))) });
                }
            }
        }
        items.push(Item { icon: IconName::Cloud, label: "Cloudflare".into(), hint: tr("palette.go"), cmd: Cmd::Go(Page::Cloudflare) });
        items.push(Item { icon: IconName::Plus, label: tr("nav.add_server"), hint: tr("palette.go"), cmd: Cmd::Go(Page::AddServer) });
        items.push(Item { icon: IconName::Settings, label: tr("nav.settings"), hint: tr("palette.go"), cmd: Cmd::Go(Page::Settings) });
        items.push(Item { icon: IconName::Download, label: tr("nav.updates"), hint: tr("palette.go"), cmd: Cmd::Go(Page::Updates) });
        for (pref, key, ic) in [(ThemePref::Light, "palette.light", IconName::Sun), (ThemePref::Dark, "palette.dark", IconName::Moon), (ThemePref::System, "palette.system", IconName::Monitor)] {
            items.push(Item { icon: ic, label: tr(key), hint: tr("palette.run"), cmd: Cmd::Theme(pref) });
        }
        if q.is_empty() {
            return items;
        }
        // Every word must appear somewhere in the label or hint.
        items.retain(|it| {
            let hay = format!("{} {}", it.label, it.hint).to_lowercase();
            q.split_whitespace().all(|w| hay.contains(w))
        });
        items
    }

    fn palette_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sel) = self.palette.as_ref().map(|p| p.selected) else { return };
        let Some(item) = self.palette_items(cx).into_iter().nth(sel) else { return };
        self.palette_exec(item.cmd, window, cx);
    }

    fn palette_exec(&mut self, cmd: Cmd, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = None;
        match cmd {
            Cmd::Go(page) => self.navigate(page, cx),
            Cmd::Rediscover(id) => self.discover(id, cx),
            Cmd::Theme(pref) => self.set_theme(pref, window, cx),
        }
        cx.notify();
    }

    fn palette_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        let n = self.palette_items(cx).len();
        if let Some(p) = self.palette.as_mut() {
            if n > 0 {
                p.selected = (p.selected as isize + delta).rem_euclid(n as isize) as usize;
            }
        }
        cx.notify();
    }

    pub(crate) fn render_palette(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let p = self.palette.as_ref()?;
        let g = Glass::get(cx).clone();
        let items = self.palette_items(cx);
        let selected = p.selected.min(items.len().saturating_sub(1));
        let mut list = v_flex().gap_0p5();
        // Keep the selected row in view by showing a window of rows around it.
        let start = selected.saturating_sub(9);
        for (i, it) in items.into_iter().enumerate().skip(start).take(12) {
            let cmd = it.cmd.clone();
            list = list.child(
                h_flex()
                    .id(SharedString::from(format!("pal-{i}")))
                    .gap_3()
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .when(i == selected, |d| d.bg(g.panel_hover))
                    .hover(|d| d.bg(g.panel_hover))
                    .child(icon(it.icon, if i == selected { g.accent } else { g.dim }))
                    .child(div().flex_1().child(it.label))
                    .child(div().text_xs().text_color(g.faint).child(it.hint))
                    .on_click(cx.listener(move |this, _, window, cx| this.palette_exec(cmd.clone(), window, cx))),
            );
        }
        Some(
            div()
                .id("palette-scrim")
                .absolute()
                .size_full()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(90.))
                .bg(gpui_kit::hsla(0., 0., 0., 0.25))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.palette = None;
                    cx.notify();
                }))
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| match ev.keystroke.key.as_str() {
                    "escape" => {
                        this.palette = None;
                        cx.notify();
                    }
                    "down" => this.palette_move(1, cx),
                    "up" => this.palette_move(-1, cx),
                    _ => {}
                }))
                .child(
                    panel(cx)
                        .id("palette")
                        .w(px(560.))
                        .max_h(px(480.))
                        .p_2()
                        .gap_2()
                        .bg(if g.dark { gpui_kit::hsla(250. / 360., 0.2, 0.11, 0.98) } else { gpui_kit::hsla(40. / 360., 0.4, 0.99, 0.98) })
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(Input::new(&p.input))
                        .child(list)
                        .child(div().px_3().text_xs().text_color(g.faint).child(tr("palette.keys"))),
                ),
        )
    }
}
