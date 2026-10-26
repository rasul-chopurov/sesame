//! Running plans: the confirmation dialog, the live job drawer, the audit
//! log entry and saving revealed credentials to the Keychain.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use sesame_core::ops::Risk;
use sesame_core::{AuditEntry, ServerId, Store as _, secrets};
use sesame_ops::{Event, Plan};
use sesame_provision::Service;

use super::theme::Glass;
use super::widgets::*;
use super::SesameApp;
use crate::i18n::{tr, trp};
use crate::runtime;

/// How to rebuild a plan when the user flips an option in the dialog.
#[derive(Clone, Copy)]
pub enum Rebuild {
    Uninstall(Service),
}

pub struct Pending {
    pub server: ServerId,
    pub plan: Plan,
    pub confirm: Entity<InputState>,
    pub rebuild: Option<Rebuild>,
    pub delete_data: bool,
}

#[derive(Clone, PartialEq)]
pub enum JobState {
    Running,
    Done,
    Failed(String),
}

pub struct Job {
    pub server: ServerId,
    pub title: String,
    pub steps: Vec<String>,
    pub current: usize,
    pub lines: Vec<String>,
    pub state: JobState,
    pub reveal: Vec<(String, String)>,
}

impl SesameApp {
    /// Shows the confirmation dialog for a plan (or the error building it).
    pub(crate) fn request(
        &mut self,
        server: ServerId,
        plan: anyhow::Result<Plan>,
        rebuild: Option<Rebuild>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match plan {
            Ok(plan) => {
                let confirm = cx.new(|cx| InputState::new(window, cx));
                self.pending = Some(Pending { server, plan, confirm, rebuild, delete_data: false });
            }
            Err(e) => {
                self.job = Some(Job {
                    server,
                    title: e.to_string(),
                    steps: vec![],
                    current: 0,
                    lines: vec![],
                    state: JobState::Failed(e.to_string()),
                    reveal: vec![],
                });
            }
        }
        cx.notify();
    }

    /// Opens the confirmation dialog for `plan` on the first server (screenshots, demos).
    #[doc(hidden)]
    pub fn demo_confirm(&mut self, plan: Plan, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.servers.first().map(|s| s.id) else { return };
        self.request(id, Ok(plan), None, window, cx);
    }

    /// Shows a finished job with output and revealed credentials (screenshots, demos).
    #[doc(hidden)]
    pub fn demo_job(&mut self, plan: &Plan, lines: Vec<String>, cx: &mut Context<Self>) {
        let Some(id) = self.servers.first().map(|s| s.id) else { return };
        self.pending = None;
        self.job = Some(Job {
            server: id,
            title: plan.title.clone(),
            steps: plan.steps.iter().map(|s| s.title.clone()).collect(),
            current: plan.steps.len(),
            lines,
            state: JobState::Done,
            reveal: plan.reveal.clone(),
        });
        cx.notify();
    }

    fn toggle_delete_data(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.pending.as_mut() else { return };
        p.delete_data = !p.delete_data;
        if let Some(Rebuild::Uninstall(svc)) = p.rebuild {
            if let Ok(plan) = sesame_provision::uninstall(svc, !p.delete_data) {
                p.plan = plan;
            }
        }
        cx.notify();
    }

    fn run_pending(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.pending.take() else { return };
        let Some(server) = self.servers.iter().find(|s| s.id == p.server).cloned() else { return };
        let plan = p.plan;
        self.job = Some(Job {
            server: server.id,
            title: plan.title.clone(),
            steps: plan.steps.iter().map(|s| s.title.clone()).collect(),
            current: 0,
            lines: vec![],
            state: JobState::Running,
            reveal: vec![],
        });
        cx.notify();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
        let run_plan = plan.clone();
        let handle = runtime::get().spawn(async move {
            let session = sesame_ssh::Session::connect(&server).await?;
            let out = sesame_ops::execute(&session, &run_plan, Some(tx)).await;
            let _ = session.close().await;
            out
        });
        let server_id = p.server;
        cx.spawn(async move |this, cx| {
            while let Some(event) = rx.recv().await {
                let alive = this
                    .update(cx, |this, cx| {
                        if let Some(job) = this.job.as_mut() {
                            match event {
                                Event::Step { index, .. } => job.current = index,
                                Event::Line(line) => {
                                    job.lines.push(line);
                                    if job.lines.len() > 400 {
                                        job.lines.drain(..100);
                                    }
                                }
                                Event::Done { .. } => {}
                            }
                        }
                        cx.notify();
                    })
                    .is_ok();
                if !alive {
                    return;
                }
            }
            let result = handle.await;
            this.update(cx, |this, cx| {
                let (ok, log, state, reveal) = match result {
                    Ok(Ok(out)) => (true, out.tail, JobState::Done, out.reveal),
                    Ok(Err(e)) => (false, format!("{e:#}"), JobState::Failed(format!("{e:#}")), vec![]),
                    Err(e) => (false, e.to_string(), JobState::Failed(e.to_string()), vec![]),
                };
                for (name, value) in &reveal {
                    let _ = secrets::set_named(server_id, name, value);
                }
                let _ = this.store.log_audit(&AuditEntry {
                    at: sesame_discovery::parse::app::now_rfc3339(),
                    server_id,
                    op: plan.op.to_string(),
                    title: plan.title.clone(),
                    actor: "user".into(),
                    ok,
                    log,
                });
                if let Some(job) = this.job.as_mut() {
                    job.state = state;
                    job.reveal = reveal;
                    job.current = job.steps.len();
                }
                if ok {
                    this.discover(server_id, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn render_confirm(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let p = self.pending.as_ref()?;
        let g = Glass::get(cx).clone();
        let destructive = p.plan.risk == Risk::Destructive;
        let typed = p.confirm.read(cx).value().trim().to_string();
        let can_run = p.plan.confirm_word.as_ref().is_none_or(|w| *w == typed);

        let mut details = v_flex().gap_1();
        for d in &p.plan.details {
            details = details.child(h_flex().gap_2().child(div().text_color(g.accent).child("•")).child(d.clone()));
        }
        let mut steps = v_flex().gap_0p5();
        for (i, s) in p.plan.steps.iter().enumerate() {
            steps = steps.child(h_flex().gap_2().text_xs().text_color(g.dim).child(mono(format!("{}.", i + 1))).child(s.title.clone()));
        }
        let dialog = panel(cx)
            .w(px(560.))
            .p_5()
            .gap_4()
            .bg(if g.dark { gpui_kit::hsla(250. / 360., 0.2, 0.11, 0.98) } else { gpui_kit::hsla(40. / 360., 0.4, 0.99, 0.98) })
            .child(
                h_flex()
                    .gap_2()
                    .child(icon(if destructive { IconName::TriangleAlert } else { IconName::Wrench }, if destructive { g.bad } else { g.accent }))
                    .child(div().text_lg().font_weight(FontWeight::BOLD).child(tr("job.confirm_title")))
                    .child(pill(
                        if destructive { tr("job.risk_destructive") } else { tr("job.risk_change") },
                        if destructive { g.bad } else { g.warn },
                    )),
            )
            .child(div().font_weight(FontWeight::SEMIBOLD).child(p.plan.title.clone()))
            .child(v_flex().gap_1().child(div().text_color(g.dim).child(tr("job.will"))).child(details))
            .child(steps)
            .when(p.rebuild.is_some(), |d| {
                d.child(
                    Switch::new("delete-data")
                        .checked(p.delete_data)
                        .label(tr("job.delete_data"))
                        .on_click(cx.listener(|this, _: &bool, _, cx| this.toggle_delete_data(cx))),
                )
            })
            .when_some(p.plan.confirm_word.clone(), |d, word| {
                d.child(
                    v_flex()
                        .gap_1()
                        .child(div().text_color(g.dim).child(trp("job.type_to_confirm", &[("word", &word)])))
                        .child(Input::new(&p.confirm)),
                )
            })
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .child(Button::new("job-cancel").ghost().label(tr("job.cancel")).on_click(cx.listener(|this, _, _, cx| {
                        this.pending = None;
                        cx.notify();
                    })))
                    .child({
                        let b = Button::new("job-run").label(tr("job.run")).disabled(!can_run);
                        let b = if destructive { b.danger() } else { b.primary() };
                        b.on_click(cx.listener(|this, _, _, cx| this.run_pending(cx)))
                    }),
            );
        Some(
            div()
                .id("confirm-overlay")
                .absolute()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(gpui_kit::black().opacity(0.35))
                .on_click(|_, _, _| {})
                .child(dialog),
        )
    }

    pub(crate) fn render_job(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let job = self.job.as_ref()?;
        let g = Glass::get(cx).clone();
        let (status, color) = match &job.state {
            JobState::Running => (tr("job.running"), g.warn),
            JobState::Done => (tr("job.done"), g.ok),
            JobState::Failed(_) => (tr("job.failed"), g.bad),
        };
        let mut steps = h_flex().gap_3().flex_wrap();
        for (i, s) in job.steps.iter().enumerate() {
            let (ic, c) = if i < job.current || job.state == JobState::Done {
                (IconName::Check, g.ok)
            } else if i == job.current && job.state == JobState::Running {
                (IconName::RefreshCw, g.warn)
            } else if i == job.current {
                (IconName::CircleAlert, g.bad)
            } else {
                (IconName::Clock, g.faint)
            };
            steps = steps.child(h_flex().gap_1().text_xs().child(icon(ic, c).size_3()).child(s.clone()));
        }
        let mut log = v_flex().gap_0p5();
        for line in job.lines.iter().rev().take(200).collect::<Vec<_>>().into_iter().rev() {
            log = log.child(mono(line.clone()).text_color(g.dim));
        }
        if let JobState::Failed(e) = &job.state {
            log = log.child(mono(e.clone()).text_color(g.bad));
        }

        let reveal = (!job.reveal.is_empty()).then(|| {
            let env: String = job.reveal.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
            let mut col = v_flex()
                .gap_1()
                .p_3()
                .rounded_md()
                .bg(g.accent_soft)
                .child(h_flex().gap_2().child(icon(IconName::KeyRound, g.accent)).child(div().text_xs().child(tr("job.reveal"))));
            for (k, v) in &job.reveal {
                col = col.child(h_flex().gap_2().child(div().w(px(220.)).child(mono(k.clone()).text_color(g.dim))).child(mono(v.clone())));
            }
            col.child(h_flex().child(
                Button::new("copy-env")
                    .ghost()
                    .icon(IconName::Copy)
                    .label(tr("job.copy_env"))
                    .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(env.clone()))),
            ))
        });

        Some(
            v_flex()
                .border_t_1()
                .border_color(g.line)
                .p_3()
                .gap_2()
                .max_h(px(320.))
                .child(
                    h_flex()
                        .gap_2()
                        .child(icon(IconName::Terminal, g.dim))
                        .child(div().flex_1().font_weight(FontWeight::SEMIBOLD).child(format!(
                            "{} · {}",
                            self.servers.iter().find(|s| s.id == job.server).map(|s| s.name.as_str()).unwrap_or(""),
                            job.title
                        )))
                        .child(pill(status, color))
                        .when(job.state != JobState::Running, |d| {
                            d.child(Button::new("job-close").ghost().label(tr("job.close")).on_click(cx.listener(|this, _, _, cx| {
                                this.job = None;
                                cx.notify();
                            })))
                        }),
                )
                .child(steps)
                .children(reveal)
                .child(div().id("job-log").flex_1().min_h(px(60.)).overflow_y_scroll().child(log)),
        )
    }
}
