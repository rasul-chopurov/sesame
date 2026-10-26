//! Agent approvals and the Agents settings card.

use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use sesame_core::{Approval, Store as _};

use super::theme::Glass;
use super::widgets::*;
use super::SesameApp;
use crate::i18n::{tr, trp};

impl SesameApp {
    /// Polls the approvals queue the MCP and REST servers write to.
    pub(crate) fn watch_approvals(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(1000)).await;
            let alive = this
                .update(cx, |this, cx| {
                    let pending = this.store.pending_approvals().unwrap_or_default();
                    if pending != this.approvals {
                        this.approvals = pending;
                        cx.notify();
                    }
                })
                .is_ok();
            if !alive {
                break;
            }
        })
        .detach();
    }

    fn decide(&mut self, id: String, allow: bool, cx: &mut Context<Self>) {
        let _ = self.store.decide_approval(&id, if allow { "approved" } else { "denied" });
        self.approvals.retain(|a| a.id != id);
        cx.notify();
    }

    pub(crate) fn render_approvals(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let first: Approval = self.approvals.first()?.clone();
        let g = Glass::get(cx).clone();
        let destructive = first.risk == "destructive";
        let server = self.servers.iter().find(|s| s.id == first.server_id).map(|s| s.name.clone());
        let mut details = v_flex().gap_1();
        for d in &first.details {
            details = details.child(h_flex().gap_2().child(div().text_color(g.accent).child("•")).child(d.clone()));
        }
        let (id_a, id_d) = (first.id.clone(), first.id.clone());
        let agent = first.actor.trim_start_matches("agent:").to_string();
        Some(
            div().absolute().top(px(52.)).right(px(16.)).w(px(440.)).child(
                panel(cx)
                    .p_4()
                    .gap_3()
                    .bg(if g.dark { gpui_kit::hsla(250. / 360., 0.2, 0.11, 0.98) } else { gpui_kit::hsla(40. / 360., 0.4, 0.99, 0.98) })
                    .border_color(if destructive { g.bad } else { g.accent })
                    .child(
                        h_flex()
                            .gap_2()
                            .child(icon(IconName::Bot, g.accent))
                            .child(div().flex_1().font_weight(FontWeight::SEMIBOLD).child(trp("approve.title", &[("agent", &agent)])))
                            .when(self.approvals.len() > 1, |d| d.child(pill(format!("+{}", self.approvals.len() - 1), g.dim)))
                            .child(pill(
                                if destructive { tr("job.risk_destructive") } else { tr("job.risk_change") },
                                if destructive { g.bad } else { g.warn },
                            )),
                    )
                    .child(div().font_weight(FontWeight::MEDIUM).child(first.title.clone()))
                    .when_some(server, |d, s| d.child(div().text_xs().text_color(g.dim).child(s)))
                    .child(details)
                    .child(
                        h_flex()
                            .gap_2()
                            .justify_end()
                            .child(Button::new("approve-deny").ghost().label(tr("approve.deny")).on_click(cx.listener(move |this, _, _, cx| this.decide(id_d.clone(), false, cx))))
                            .child({
                                let b = Button::new("approve-allow").label(tr("approve.allow"));
                                let b = if destructive { b.danger() } else { b.primary() };
                                b.on_click(cx.listener(move |this, _, _, cx| this.decide(id_a.clone(), true, cx)))
                            }),
                    ),
            ),
        )
    }
}

pub fn settings_card(app: &SesameApp, cx: &mut Context<SesameApp>) -> Div {
    let g = Glass::get(cx).clone();
    let exe = sesame_update::installed_location()
        .map(|p| p.join("Contents/MacOS/sesame"))
        .or_else(|| std::env::current_exe().ok())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "sesame".into());
    let claude = format!("claude mcp add sesame -- \"{exe}\" mcp");
    let json = format!("{{\n  \"mcpServers\": {{\n    \"sesame\": {{ \"command\": \"{exe}\", \"args\": [\"mcp\"] }}\n  }}\n}}");
    let block = |id: &'static str, label: String, text: String| {
        let copy = text.clone();
        v_flex()
            .gap_1()
            .child(div().text_xs().text_color(g.dim).child(label))
            .child(
                h_flex()
                    .gap_2()
                    .items_start()
                    .child(div().flex_1().p_2().rounded_md().bg(g.panel_hover).child(mono(text)))
                    .child(Button::new(id).ghost().icon(IconName::Copy).on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.clone())))),
            )
    };
    let share = app.store.setting_or("agents.share_secrets", "false") == "true";
    card(&tr("agents.title"), cx)
        .gap_3()
        .child(div().text_color(g.dim).child(tr("agents.lead")))
        .child(block("copy-claude", tr("agents.claude_code"), claude))
        .child(block("copy-json", tr("agents.json"), json))
        .child(div().text_xs().text_color(g.faint).child(tr("agents.password_note")))
        .child(
            Switch::new("agents-share")
                .checked(share)
                .label(tr("agents.share"))
                .on_click(cx.listener(|this, v: &bool, _, cx| {
                    let _ = this.store.set_setting("agents.share_secrets", if *v { "true" } else { "false" });
                    cx.notify();
                })),
        )
        .child(
            v_flex()
                .gap_1()
                .child(div().font_weight(FontWeight::MEDIUM).child(tr("agents.rest")))
                .child(div().text_xs().text_color(g.faint).child(tr("agents.rest_hint")))
                .child(
                    h_flex()
                        .gap_2()
                        .child(Button::new("api-token").ghost().icon(IconName::KeyRound).label(tr("agents.new_token")).on_click(cx.listener(|this, _, _, cx| {
                            this.api_token = crate::rest::new_token(&this.store).ok();
                            cx.notify();
                        })))
                        .when_some(app.api_token.clone(), |d, t| {
                            let copy = t.clone();
                            d.child(mono(t)).child(Button::new("copy-token").ghost().icon(IconName::Copy).on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))))
                        }),
                ),
        )
}
