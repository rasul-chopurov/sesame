//! Sesame Glass: a translucent light and dark theme over a blurred window,
//! with a warm saffron accent and a soft carpet-coloured glow behind.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Global, Hsla, Window, WindowAppearance, px};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemePref {
    System,
    Light,
    Dark,
}

impl ThemePref {
    pub fn parse(s: &str) -> Self {
        match s {
            "light" => ThemePref::Light,
            "dark" => ThemePref::Dark,
            _ => ThemePref::System,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ThemePref::System => "system",
            ThemePref::Light => "light",
            ThemePref::Dark => "dark",
        }
    }
}

/// Colours Sesame draws with directly (gpui-component widgets read `Theme`).
#[derive(Debug, Clone)]
pub struct Glass {
    pub dark: bool,
    /// Window fill under everything; translucent when the window blurs.
    pub window: Hsla,
    /// Two corner glows of the background gradient.
    pub glow_a: Hsla,
    pub glow_b: Hsla,
    pub panel: Hsla,
    pub panel_hover: Hsla,
    pub panel_active: Hsla,
    pub line: Hsla,
    pub text: Hsla,
    pub dim: Hsla,
    pub faint: Hsla,
    pub accent: Hsla,
    pub accent_text: Hsla,
    pub accent_soft: Hsla,
    pub ok: Hsla,
    pub warn: Hsla,
    pub bad: Hsla,
    pub info: Hsla,
}

impl Global for Glass {}

fn c(h: f32, s: f32, l: f32, a: f32) -> Hsla {
    gpui_kit::hsla(h / 360.0, s, l, a)
}

impl Glass {
    pub fn new(dark: bool, translucent: bool) -> Self {
        if dark {
            Glass {
                dark,
                window: c(250.0, 0.20, 0.075, if translucent { 0.80 } else { 1.0 }),
                glow_a: c(232.0, 0.55, 0.30, 0.30),
                glow_b: c(14.0, 0.70, 0.36, 0.26),
                panel: c(0.0, 0.0, 1.0, 0.035),
                panel_hover: c(0.0, 0.0, 1.0, 0.07),
                panel_active: c(36.0, 0.90, 0.60, 0.16),
                line: c(0.0, 0.0, 1.0, 0.085),
                text: c(40.0, 0.25, 0.93, 1.0),
                dim: c(40.0, 0.08, 0.66, 1.0),
                faint: c(40.0, 0.05, 0.46, 1.0),
                accent: c(36.0, 0.92, 0.60, 1.0),
                accent_text: c(250.0, 0.30, 0.10, 1.0),
                accent_soft: c(36.0, 0.92, 0.60, 0.14),
                ok: c(145.0, 0.55, 0.52, 1.0),
                warn: c(40.0, 0.95, 0.56, 1.0),
                bad: c(4.0, 0.80, 0.62, 1.0),
                info: c(205.0, 0.80, 0.62, 1.0),
            }
        } else {
            Glass {
                dark,
                window: c(40.0, 0.40, 0.975, if translucent { 0.84 } else { 1.0 }),
                glow_a: c(38.0, 0.95, 0.80, 0.42),
                glow_b: c(350.0, 0.80, 0.88, 0.36),
                panel: c(0.0, 0.0, 1.0, 0.62),
                panel_hover: c(30.0, 0.30, 0.20, 0.05),
                panel_active: c(30.0, 0.90, 0.50, 0.13),
                line: c(30.0, 0.25, 0.20, 0.10),
                text: c(250.0, 0.25, 0.13, 1.0),
                dim: c(250.0, 0.08, 0.40, 1.0),
                faint: c(250.0, 0.05, 0.58, 1.0),
                accent: c(26.0, 0.88, 0.50, 1.0),
                accent_text: c(0.0, 0.0, 1.0, 1.0),
                accent_soft: c(26.0, 0.88, 0.50, 0.12),
                ok: c(145.0, 0.60, 0.36, 1.0),
                warn: c(34.0, 0.92, 0.45, 1.0),
                bad: c(2.0, 0.72, 0.50, 1.0),
                info: c(210.0, 0.75, 0.48, 1.0),
            }
        }
    }

    pub fn get(cx: &App) -> &Glass {
        cx.global::<Glass>()
    }
}

pub fn is_dark(pref: ThemePref, window: Option<&Window>) -> bool {
    match pref {
        ThemePref::Light => false,
        ThemePref::Dark => true,
        ThemePref::System => window.is_some_and(|w| {
            matches!(w.appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark)
        }),
    }
}

/// Installs the theme for both Sesame and gpui-component widgets.
pub fn apply(dark: bool, translucent: bool, cx: &mut App) {
    Theme::change(if dark { ThemeMode::Dark } else { ThemeMode::Light }, None, cx);
    let g = Glass::new(dark, translucent);
    let solid_panel = if dark { c(250.0, 0.18, 0.12, 1.0) } else { c(40.0, 0.30, 0.99, 1.0) };
    let gg = g.clone();
    // `update` re-derives the tokens components paint with.
    Theme::update(cx, move |t| {
        let g = gg;
        t.font_size = px(14.);
        t.radius = px(7.);
        t.mono_font_family = "Menlo".into();

        let k = &mut t.colors;
        k.background = g.window;
        k.foreground = g.text;
        k.muted = g.panel;
        k.muted_foreground = g.dim;
        k.border = g.line;
        k.input = g.line;
        k.ring = g.accent;
        k.caret = g.accent;
        k.selection = g.accent_soft;
        k.accent = g.panel_hover;
        k.accent_foreground = g.text;
        k.primary = g.accent;
        k.primary_hover = g.accent.opacity(0.9);
        k.primary_active = g.accent.opacity(0.8);
        k.primary_foreground = g.accent_text;
        k.button_primary = g.accent;
        k.button_primary_hover = g.accent.opacity(0.9);
        k.button_primary_active = g.accent.opacity(0.8);
        k.button_primary_foreground = g.accent_text;
        k.secondary = g.panel;
        k.secondary_hover = g.panel_hover;
        k.secondary_active = g.panel_active;
        k.secondary_foreground = g.text;
        k.button = g.panel;
        k.button_hover = g.panel_hover;
        k.button_active = g.panel_active;
        k.button_foreground = g.text;
        k.popover = solid_panel;
        k.popover_foreground = g.text;
        k.list = gpui_kit::transparent_black();
        k.list_hover = g.panel_hover;
        k.list_active = g.panel_active;
        k.sidebar = gpui_kit::transparent_black();
        k.sidebar_border = g.line;
        k.title_bar = gpui_kit::transparent_black();
        k.title_bar_border = gpui_kit::transparent_black();
        k.status_bar = gpui_kit::transparent_black();
        k.status_bar_border = g.line;
        k.success = g.ok;
        k.warning = g.warn;
        k.danger = g.bad;
        k.info = g.info;
        k.link = g.accent;
        k.switch = g.line;
        k.scrollbar_thumb = g.line;
    });
    cx.set_global(g);
}
