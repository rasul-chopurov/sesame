//! A file manager confined to one site's directory (SFTP).

use std::sync::Arc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Editor, EditorState, Input, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use sesame_core::ServerId;
use sesame_ssh::files::{Entry, SiteFiles, parent};

use super::theme::Glass;
use super::widgets::*;
use super::{Page, Section, SesameApp};
use crate::format;
use crate::i18n::{tr, trp};
use crate::runtime;

const MAX_EDIT: u64 = 512 * 1024;

pub struct FilesState {
    pub server: ServerId,
    pub site: usize,
    pub root: String,
    pub cwd: String,
    pub client: Option<Arc<SiteFiles>>,
    pub entries: Vec<Entry>,
    pub loading: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub editing: Option<(String, Entity<EditorState>)>,
    pub confirm_delete: Option<String>,
}

pub struct Inputs {
    pub new_folder: Entity<InputState>,
    pub filter: Entity<InputState>,
}

impl Inputs {
    pub fn new(window: &mut Window, cx: &mut Context<SesameApp>) -> Self {
        Self {
            new_folder: cx.new(|cx| InputState::new(window, cx).placeholder("new-folder")),
            filter: cx.new(|cx| InputState::new(window, cx).placeholder(tr("files.filter"))),
        }
    }
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") }
}

impl SesameApp {
    pub(crate) fn open_files(&mut self, server: ServerId, site: usize, root: String, cx: &mut Context<Self>) {
        let same = self.files.as_ref().is_some_and(|f| f.server == server && f.root == root && f.client.is_some());
        if !same {
            self.files = Some(FilesState {
                server,
                site,
                root: root.clone(),
                cwd: String::new(),
                client: None,
                entries: vec![],
                loading: true,
                error: None,
                notice: None,
                editing: None,
                confirm_delete: None,
            });
            let Some(config) = self.servers.iter().find(|s| s.id == server).cloned() else { return };
            let job = runtime::get().spawn(async move {
                let client = Arc::new(SiteFiles::open(&config, &root).await?);
                let entries = client.list("").await?;
                anyhow::Ok((client, entries))
            });
            cx.spawn(async move |this, cx| {
                let result = job.await;
                this.update(cx, |this, cx| {
                    if let Some(f) = this.files.as_mut() {
                        f.loading = false;
                        match result {
                            Ok(Ok((client, entries))) => {
                                f.client = Some(client);
                                f.entries = entries;
                            }
                            Ok(Err(e)) => f.error = Some(format!("{e:#}")),
                            Err(e) => f.error = Some(e.to_string()),
                        }
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        self.navigate(Page::Server(server, Section::Files(site)), cx);
    }

    /// Runs a file operation, then reloads `dir_after`.
    fn files_op<T: Send + 'static>(
        &mut self,
        dir_after: String,
        op: impl FnOnce(Arc<SiteFiles>) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<T>> + Send>> + Send + 'static,
        done: impl FnOnce(&mut SesameApp, T, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(f) = self.files.as_mut() else { return };
        let Some(client) = f.client.clone() else { return };
        f.loading = true;
        f.error = None;
        f.notice = None;
        cx.notify();
        let dir = dir_after.clone();
        let job = runtime::get().spawn(async move {
            let value = op(client.clone()).await?;
            let entries = client.list(&dir).await?;
            anyhow::Ok((value, entries))
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            this.update(cx, |this, cx| {
                let outcome = match result {
                    Ok(Ok((value, entries))) => {
                        if let Some(f) = this.files.as_mut() {
                            f.loading = false;
                            f.cwd = dir_after;
                            f.entries = entries;
                        }
                        Some(value)
                    }
                    Ok(Err(e)) => {
                        if let Some(f) = this.files.as_mut() {
                            f.loading = false;
                            f.error = Some(format!("{e:#}"));
                        }
                        None
                    }
                    Err(e) => {
                        if let Some(f) = this.files.as_mut() {
                            f.loading = false;
                            f.error = Some(e.to_string());
                        }
                        None
                    }
                };
                if let Some(v) = outcome {
                    done(this, v, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn files_cd(&mut self, dir: String, cx: &mut Context<Self>) {
        self.files_op(dir, |_| Box::pin(async { Ok(()) }), |_, _, _| {}, cx);
    }

    fn files_edit(&mut self, rel: String, cx: &mut Context<Self>) {
        let cwd = self.files.as_ref().map(|f| f.cwd.clone()).unwrap_or_default();
        let r = rel.clone();
        self.files_op(
            cwd,
            move |c| Box::pin(async move { c.read(&r, MAX_EDIT).await }),
            move |this, bytes, cx| {
                let text = match String::from_utf8(bytes) {
                    Ok(t) => t,
                    Err(_) => {
                        if let Some(f) = this.files.as_mut() {
                            f.error = Some(format!("{rel} is not a text file"));
                        }
                        return;
                    }
                };
                let Some(window) = cx.windows().first().copied() else { return };
                let editor = window
                    .update(cx, |_, window, cx| cx.new(|cx| EditorState::new(window, cx).default_value(text)))
                    .ok();
                if let (Some(f), Some(editor)) = (this.files.as_mut(), editor) {
                    f.editing = Some((rel, editor));
                }
            },
            cx,
        );
    }

    fn files_save(&mut self, cx: &mut Context<Self>) {
        let Some(f) = self.files.as_ref() else { return };
        let Some((rel, editor)) = f.editing.clone() else { return };
        let data = editor.read(cx).value().to_string().into_bytes();
        let cwd = f.cwd.clone();
        self.files_op(
            cwd,
            move |c| Box::pin(async move { c.write(&rel, &data).await }),
            |this, _, _| {
                if let Some(f) = this.files.as_mut() {
                    f.notice = Some(tr("files.saved"));
                }
            },
            cx,
        );
    }

    fn files_upload(&mut self, cx: &mut Context<Self>) {
        let cwd = self.files.as_ref().map(|f| f.cwd.clone()).unwrap_or_default();
        let picked = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: None });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else { return };
            this.update(cx, |this, cx| {
                let dir = cwd.clone();
                this.files_op(
                    cwd,
                    move |c| {
                        Box::pin(async move {
                            for p in paths {
                                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                                let data = tokio::fs::read(&p).await?;
                                c.write(&join(&dir, &name), &data).await?;
                            }
                            Ok(())
                        })
                    },
                    |_, _, _| {},
                    cx,
                );
            })
            .ok();
        })
        .detach();
    }

    fn files_download(&mut self, rel: String, cx: &mut Context<Self>) {
        let name = rel.rsplit('/').next().unwrap_or("file").to_string();
        let downloads = std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Downloads")).unwrap_or_default();
        let picked = cx.prompt_for_new_path(&downloads, Some(&name));
        let cwd = self.files.as_ref().map(|f| f.cwd.clone()).unwrap_or_default();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(target))) = picked.await else { return };
            this.update(cx, |this, cx| {
                let r = rel.clone();
                this.files_op(
                    cwd,
                    move |c| {
                        Box::pin(async move {
                            let data = c.read(&r, 256 * 1024 * 1024).await?;
                            tokio::fs::write(&target, data).await?;
                            Ok(target.display().to_string())
                        })
                    },
                    |this, path, _| {
                        if let Some(f) = this.files.as_mut() {
                            f.notice = Some(path);
                        }
                    },
                    cx,
                );
            })
            .ok();
        })
        .detach();
    }
}

pub fn view(app: &SesameApp, server: ServerId, site_idx: usize, cx: &mut Context<SesameApp>) -> Div {
    let g = Glass::get(cx).clone();
    let Some(f) = app.files.as_ref().filter(|f| f.server == server && f.site == site_idx) else {
        return v_flex().child(empty(tr("cf.loading"), cx));
    };
    let back = h_flex()
        .id("files-back")
        .gap_1()
        .cursor_pointer()
        .text_color(g.dim)
        .hover(|d| d.text_color(g.text))
        .on_click(cx.listener(move |this, _, _, cx| this.navigate(Page::Server(server, Section::Site(site_idx)), cx)))
        .child("←")
        .child(tr("sites.back"));

    // Breadcrumbs.
    let mut crumbs = h_flex().gap_1().child(
        div()
            .id("crumb-root")
            .cursor_pointer()
            .text_color(g.accent)
            .child(mono(f.root.clone()))
            .on_click(cx.listener(|this, _, _, cx| this.files_cd(String::new(), cx))),
    );
    let mut acc = String::new();
    for (i, part) in f.cwd.split('/').filter(|p| !p.is_empty()).enumerate() {
        acc = join(&acc, part);
        let target = acc.clone();
        crumbs = crumbs.child(div().text_color(g.faint).child("/")).child(
            div()
                .id(SharedString::from(format!("crumb-{i}")))
                .cursor_pointer()
                .hover(|d| d.text_color(g.accent))
                .child(mono(part.to_string()))
                .on_click(cx.listener(move |this, _, _, cx| this.files_cd(target.clone(), cx))),
        );
    }

    let toolbar = h_flex()
        .gap_2()
        .child(div().flex_1().min_w_0().overflow_hidden().child(crumbs))
        .child(div().w(px(220.)).child(Input::new(&app.files_in.filter).prefix(icon(IconName::Search, g.faint)).cleanable(true)))
        .child(div().w(px(160.)).child(Input::new(&app.files_in.new_folder)))
        .child(
            Button::new("files-mkdir")
                .ghost()
                .icon(IconName::Plus)
                .label(tr("files.new_folder"))
                .on_click(cx.listener(|this, _, _, cx| {
                    let name = this.files_in.new_folder.read(cx).value().trim().to_string();
                    let Some(cwd) = this.files.as_ref().map(|f| f.cwd.clone()) else { return };
                    if name.is_empty() || name.contains('/') {
                        return;
                    }
                    let rel = join(&cwd, &name);
                    this.files_op(cwd, move |c| Box::pin(async move { c.mkdir(&rel).await }), |_, _, _| {}, cx);
                })),
        )
        .child(
            Button::new("files-upload")
                .primary()
                .icon(IconName::Download)
                .label(tr("files.upload"))
                .on_click(cx.listener(|this, _, _, cx| this.files_upload(cx))),
        )
        .child(
            Button::new("files-refresh")
                .ghost()
                .loading(f.loading)
                .icon(IconName::RefreshCw)
                .on_click(cx.listener(|this, _, _, cx| {
                    let cwd = this.files.as_ref().map(|f| f.cwd.clone()).unwrap_or_default();
                    this.files_cd(cwd, cx);
                })),
        );

    let q = app.files_in.filter.read(cx).value().trim().to_lowercase();
    let head = h_flex()
        .gap_3()
        .px_2()
        .pb_1()
        .text_xs()
        .text_color(g.faint)
        .child(div().flex_1().child(tr("db.name").to_uppercase()))
        .child(div().w(px(100.)).child(tr("files.size").to_uppercase()))
        .child(div().w(px(60.)).child("MODE"))
        .child(div().w(px(220.)));
    let mut table = v_flex().gap_0p5().child(head);
    if !f.cwd.is_empty() {
        let up = parent(&f.cwd);
        table = table.child(
            h_flex()
                .id("files-up")
                .gap_2()
                .min_h(px(30.))
                .px_2()
                .rounded_md()
                .cursor_pointer()
                .hover(|d| d.bg(g.panel_hover))
                .on_click(cx.listener(move |this, _, _, cx| this.files_cd(up.clone(), cx)))
                .child(icon(IconName::Folder, g.faint))
                .child(mono("..")),
        );
    }
    let entries: Vec<&Entry> = f.entries.iter().filter(|e| q.is_empty() || e.name.to_lowercase().contains(&q)).collect();
    if entries.is_empty() && !f.loading {
        table = table.child(empty(tr("files.empty"), cx));
    }
    for e in entries {
        let rel = join(&f.cwd, &e.name);
        let confirming = f.confirm_delete.as_deref() == Some(rel.as_str());
        let (r_open, r_dl, r_del, r_yes) = (rel.clone(), rel.clone(), rel.clone(), rel.clone());
        let is_dir = e.is_dir;
        let mut actions = h_flex().w(px(220.)).gap_1().justify_end();
        if confirming {
            actions = actions
                .child(div().text_xs().text_color(g.bad).child(trp("files.confirm_delete", &[("name", &e.name)])))
                .child(Button::new(SharedString::from(format!("yes-{rel}"))).xsmall().danger().label(tr("files.yes")).on_click(cx.listener(move |this, _, _, cx| {
                    let cwd = this.files.as_ref().map(|f| f.cwd.clone()).unwrap_or_default();
                    let r = r_yes.clone();
                    if let Some(f) = this.files.as_mut() {
                        f.confirm_delete = None;
                    }
                    this.files_op(cwd, move |c| Box::pin(async move { c.remove(&r).await }), |_, _, _| {}, cx);
                })))
                .child(Button::new(SharedString::from(format!("no-{rel}"))).xsmall().ghost().label(tr("job.cancel")).on_click(cx.listener(|this, _, _, cx| {
                    if let Some(f) = this.files.as_mut() {
                        f.confirm_delete = None;
                    }
                    cx.notify();
                })));
        } else {
            if !is_dir {
                actions = actions.child(
                    Button::new(SharedString::from(format!("dl-{rel}")))
                        .xsmall()
                        .ghost()
                        .icon(IconName::Download)
                        .tooltip(tr("files.download"))
                        .on_click(cx.listener(move |this, _, _, cx| this.files_download(r_dl.clone(), cx))),
                );
            }
            actions = actions.child(
                Button::new(SharedString::from(format!("rm-{rel}")))
                    .xsmall()
                    .ghost()
                    .icon(IconName::X)
                    .tooltip(tr("files.delete"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(f) = this.files.as_mut() {
                            f.confirm_delete = Some(r_del.clone());
                        }
                        cx.notify();
                    })),
            );
        }
        table = table.child(
            h_flex()
                .gap_3()
                .min_h(px(32.))
                .px_2()
                .rounded_md()
                .border_b_1()
                .border_color(g.line)
                .hover(|d| d.bg(g.panel_hover))
                .child(
                    h_flex()
                        .id(SharedString::from(format!("open-{rel}")))
                        .flex_1()
                        .min_w_0()
                        .gap_2()
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if is_dir {
                                this.files_cd(r_open.clone(), cx);
                            } else {
                                this.files_edit(r_open.clone(), cx);
                            }
                        }))
                        .child(icon(if e.is_dir { IconName::Folder } else { IconName::FileText }, if e.is_dir { g.accent } else { g.faint }))
                        .child(mono(e.name.clone()).when(e.is_link, |d| d.italic())),
                )
                .child(div().w(px(100.)).text_xs().text_color(g.dim).child(if e.is_dir { "—".into() } else { format::bytes(e.size) }))
                .child(div().w(px(60.)).child(mono(format!("{:o}", e.mode)).text_color(g.faint)))
                .child(actions),
        );
    }

    let editor = f.editing.as_ref().map(|(rel, state)| {
        card(rel, cx)
            .child(div().h(px(420.)).child(Editor::new(state)))
            .child(
                h_flex()
                    .gap_2()
                    .child(Button::new("files-save").primary().icon(IconName::Check).label(tr("files.save")).on_click(cx.listener(|this, _, _, cx| this.files_save(cx))))
                    .child(Button::new("files-close").ghost().label(tr("files.close")).on_click(cx.listener(|this, _, _, cx| {
                        if let Some(f) = this.files.as_mut() {
                            f.editing = None;
                        }
                        cx.notify();
                    }))),
            )
    });

    v_flex()
        .gap_4()
        .child(back)
        .child(div().text_xs().text_color(g.faint).child(tr("files.lead")))
        .child(toolbar)
        .when_some(f.error.clone(), |d, e| d.child(div().text_color(g.bad).child(e)))
        .when_some(f.notice.clone(), |d, n| d.child(div().text_color(g.ok).child(n)))
        .children(editor)
        .child(card(&tr("files.title"), cx).child(table))
}

impl SesameApp {
    /// Shows the file manager with sample entries (screenshots, demos).
    #[doc(hidden)]
    pub fn demo_files(&mut self, server: ServerId, site: usize, cx: &mut Context<Self>) {
        let e = |name: &str, is_dir: bool, size: u64, mode: u32| Entry { name: name.into(), is_dir, is_link: false, size, modified: 0, mode };
        self.files = Some(FilesState {
            server,
            site,
            root: "/home/s_shop_example_com/shop.example.com".into(),
            cwd: "public".into(),
            client: None,
            entries: vec![e("build", true, 0, 0o755), e("images", true, 0, 0o755), e(".htaccess", false, 603, 0o644), e("favicon.ico", false, 4286, 0o644), e("index.php", false, 1727, 0o644), e("robots.txt", false, 24, 0o644)],
            loading: false,
            error: None,
            notice: None,
            editing: None,
            confirm_delete: Some("public/robots.txt".into()),
        });
        cx.notify();
    }
}
