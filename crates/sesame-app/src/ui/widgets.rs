//! Small building blocks shared by the views.

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use sesame_core::snapshot::Framework;

use super::theme::Glass;

pub fn icon(name: IconName, color: Hsla) -> Icon {
    Icon::new(name).size_4().text_color(color)
}

pub fn brand(slug: &str, size: Pixels) -> Option<Svg> {
    let (path, rgb) = crate::assets::brand(slug)?;
    Some(svg().path(path).size(size).flex_shrink_0().text_color(gpui_kit::rgb(rgb)))
}

pub fn framework_slug(f: Framework) -> Option<&'static str> {
    match f {
        Framework::Laravel => Some("laravel"),
        Framework::WordPress => Some("wordpress"),
        Framework::Node => Some("nodedotjs"),
        _ => None,
    }
}

/// A translucent rounded surface.
pub fn panel(cx: &App) -> Div {
    let g = Glass::get(cx);
    v_flex().rounded_lg().border_1().border_color(g.line).bg(g.panel)
}

/// Panel with a small uppercase heading.
pub fn card(title: &str, cx: &App) -> Div {
    panel(cx).p_4().gap_2().child(heading(title, cx))
}

pub fn heading(title: &str, cx: &App) -> Div {
    div()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(Glass::get(cx).faint)
        .child(title.to_uppercase())
}

pub fn kv(label: &str, value: impl Into<SharedString>, cx: &App) -> Div {
    h_flex()
        .gap_4()
        .min_h(px(24.))
        .child(div().w(px(140.)).flex_shrink_0().text_color(Glass::get(cx).dim).child(label.to_string()))
        .child(div().flex_1().min_w_0().overflow_hidden().child(value.into()))
}

pub fn mono(text: impl Into<SharedString>) -> Div {
    div().font_family("Menlo").text_xs().child(text.into())
}

/// Small coloured label: tinted background, coloured text.
pub fn pill(text: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .px_2()
        .py_0p5()
        .rounded_md()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .bg(color.opacity(0.14))
        .text_color(color)
        .child(text.into())
}

pub fn dot(color: Hsla) -> Div {
    div().size(px(7.)).flex_shrink_0().rounded_full().bg(color)
}

pub fn empty(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .p_10()
        .flex()
        .justify_center()
        .text_color(Glass::get(cx).dim)
        .child(text.into())
}

/// Process/service state → colour.
pub fn state_color(state: &str, cx: &App) -> Hsla {
    let g = Glass::get(cx);
    match state {
        "RUNNING" | "running" | "active" => g.ok,
        "STARTING" | "BACKOFF" | "inactive" | "exited" => g.warn,
        _ => g.bad,
    }
}

/// Square letter avatar tinted by the domain's hash.
pub fn avatar(text: &str, cx: &App) -> Div {
    let g = Glass::get(cx);
    let hue = text.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) % 360;
    let base = gpui_kit::hsla(hue as f32 / 360.0, 0.6, if g.dark { 0.55 } else { 0.5 }, 1.0);
    let letter = text.chars().find(|c| c.is_alphanumeric()).unwrap_or('•').to_uppercase().to_string();
    div()
        .size(px(34.))
        .flex_shrink_0()
        .rounded_lg()
        .flex()
        .items_center()
        .justify_center()
        .bg(base.opacity(0.18))
        .text_color(base)
        .font_weight(FontWeight::BOLD)
        .child(letter)
}

/// Segmented control: one option highlighted.
pub fn segmented<T: Copy + PartialEq + 'static>(
    id: &str,
    options: &[(T, String)],
    selected: T,
    on_pick: impl Fn(T, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
    let g = Glass::get(cx);
    let on_pick = std::rc::Rc::new(on_pick);
    let mut row = h_flex().p_0p5().gap_0p5().rounded_lg().bg(g.panel).border_1().border_color(g.line);
    for (i, (value, label)) in options.iter().enumerate() {
        let value = *value;
        let active = value == selected;
        let on_pick = on_pick.clone();
        row = row.child(
            div()
                .id(SharedString::from(format!("{id}-{i}")))
                .px_3()
                .py_1()
                .rounded_md()
                .cursor_pointer()
                .text_sm()
                .when(active, |d| d.bg(g.accent).text_color(g.accent_text))
                .when(!active, |d| d.text_color(g.dim).hover(|d| d.bg(g.panel_hover)))
                .on_click(move |_, window, cx| on_pick(value, window, cx))
                .child(label.clone()),
        );
    }
    row
}
