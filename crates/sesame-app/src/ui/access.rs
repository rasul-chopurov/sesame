//! The Access card on a site: its owner account and people with their own keys.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use sesame_core::ServerId;
use sesame_core::snapshot::Site;

use super::theme::Glass;
use super::widgets::*;
use super::SesameApp;
use crate::i18n::{tr, trp};

pub struct AccessInputs {
    pub person: Entity<InputState>,
    pub key: Entity<InputState>,
    pub shell: bool,
}

impl AccessInputs {
    pub fn new(window: &mut Window, cx: &mut Context<SesameApp>) -> Self {
        Self {
            person: cx.new(|cx| InputState::new(window, cx).placeholder("anna")),
            key: cx.new(|cx| InputState::new(window, cx).placeholder("ssh-ed25519 AAAA… anna@laptop")),
            shell: false,
        }
    }
}

pub fn access_card(app: &SesameApp, server: ServerId, site: &Site, cx: &mut Context<SesameApp>) -> Div {
    let g = Glass::get(cx).clone();
    let mut c = card(&tr("access.title"), cx);
    let (Some(owner), Some(app_path)) = (site.owner.clone(), site.app_path.clone()) else {
        return c.child(div().text_xs().text_color(g.faint).child(tr("access.unknown")));
    };
    let host = app.servers.iter().find(|s| s.id == server).map(|s| s.host.clone()).unwrap_or_default();
    c = c.child(
        h_flex()
            .gap_3()
            .child(icon(IconName::User, g.dim))
            .child(div().text_color(g.dim).child(tr("access.owner")))
            .child(mono(owner.clone()))
            .when(owner == "root", |d| d.child(pill(tr("access.root_owner"), g.warn))),
    );
    if owner == "root" {
        return c.child(div().text_xs().text_color(g.faint).child(tr("access.root_hint")));
    }
    let domain = site.domains.first().cloned().unwrap_or_default();
    if site.access.is_empty() {
        c = c.child(div().text_xs().text_color(g.faint).child(tr("access.nobody")));
    }
    for a in &site.access {
        let (d, p, ap) = (domain.clone(), a.name.clone(), app_path.clone());
        c = c.child(
            h_flex()
                .gap_3()
                .min_h(px(28.))
                .child(dot(g.ok))
                .child(div().w(px(120.)).font_weight(FontWeight::MEDIUM).child(a.name.clone()))
                .child(pill(a.mode.to_uppercase(), if a.mode == "shell" { g.warn } else { g.accent }))
                .child(div().flex_1().child(mono(format!("{} {owner}@{host}", if a.mode == "shell" { "ssh" } else { "sftp" }))))
                .child(div().text_xs().text_color(g.faint).child(a.key.clone()))
                .child(
                    Button::new(SharedString::from(format!("revoke-{}", a.name)))
                        .ghost()
                        .icon(IconName::X)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let plan = sesame_ops::site::revoke_access(&d, &ap, &p);
                            this.request(server, plan, None, window, cx);
                        })),
                ),
        );
    }
    let shell = app.access_in.shell;
    let (d, ap) = (domain.clone(), app_path.clone());
    c.child(
        v_flex()
            .gap_2()
            .pt_2()
            .child(div().font_weight(FontWeight::MEDIUM).child(tr("access.add")))
            .child(
                h_flex()
                    .gap_2()
                    .child(div().w(px(140.)).child(Input::new(&app.access_in.person)))
                    .child(div().flex_1().child(Input::new(&app.access_in.key))),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(Switch::new("access-shell").checked(shell).label(tr("access.shell")).on_click(cx.listener(|this, v: &bool, _, cx| {
                        this.access_in.shell = *v;
                        cx.notify();
                    })))
                    .child(div().flex_1().text_xs().text_color(g.faint).child(if shell { tr("access.shell_hint") } else { tr("access.sftp_hint") }))
                    .child(Button::new("access-grant").primary().icon(IconName::KeyRound).label(tr("access.grant")).on_click(cx.listener(move |this, _, window, cx| {
                        let person = this.access_in.person.read(cx).value().trim().to_string();
                        let key = this.access_in.key.read(cx).value().trim().to_string();
                        let plan = sesame_ops::site::grant_access(&d, &ap, &person, &key, this.access_in.shell);
                        if plan.is_ok() {
                            this.access_in.person.update(cx, |s, cx| s.set_value("", window, cx));
                            this.access_in.key.update(cx, |s, cx| s.set_value("", window, cx));
                        }
                        this.request(server, plan, None, window, cx);
                    }))),
            )
            .child(div().text_xs().text_color(g.faint).child(trp("access.scope", &[("owner", &owner)]))),
    )
}
