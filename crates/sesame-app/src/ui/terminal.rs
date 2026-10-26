//! An embedded SSH terminal: a PTY on the server, parsed with vt100 and
//! drawn as a grid of styled cells.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use sesame_core::ServerId;
use sesame_ssh::{Shell, ShellInput};
use tokio::sync::mpsc::UnboundedSender;

use super::theme::Glass;
use super::SesameApp;
use crate::i18n::{tr, trp};
use crate::runtime;

const COLS: u16 = 120;
const ROWS: u16 = 34;
const CELL_H: f32 = 17.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TermStatus {
    Connecting,
    Open,
    Closed,
}

pub struct TerminalState {
    pub server: ServerId,
    pub parser: vt100::Parser,
    pub input: Option<UnboundedSender<ShellInput>>,
    pub status: TermStatus,
    pub error: Option<String>,
    pub focus: FocusHandle,
}

impl SesameApp {
    pub(crate) fn open_terminal(&mut self, server: ServerId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(config) = self.servers.iter().find(|s| s.id == server).cloned() else { return };
        if let Some(t) = self.terminal.as_ref().filter(|t| t.server == server && t.status != TermStatus::Closed) {
            window.focus(&t.focus, cx);
            return;
        }
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.terminal = Some(TerminalState {
            server,
            parser: vt100::Parser::new(std::num::NonZeroU16::new(ROWS).unwrap(), std::num::NonZeroU16::new(COLS).unwrap(), 2000),
            input: None,
            status: TermStatus::Connecting,
            error: None,
            focus,
        });
        cx.notify();
        let job = runtime::get().spawn(async move { Shell::open(&config, COLS, ROWS).await });
        cx.spawn(async move |this, cx| {
            let shell = match job.await {
                Ok(Ok(shell)) => shell,
                Ok(Err(e)) => {
                    let _ = this.update(cx, |this, cx| {
                        if let Some(t) = this.terminal.as_mut() {
                            t.status = TermStatus::Closed;
                            t.error = Some(format!("{e:#}"));
                        }
                        cx.notify();
                    });
                    return;
                }
                Err(_) => return,
            };
            let Shell { input, mut output } = shell;
            let ok = this
                .update(cx, |this, cx| {
                    if let Some(t) = this.terminal.as_mut() {
                        t.input = Some(input);
                        t.status = TermStatus::Open;
                    }
                    cx.notify();
                })
                .is_ok();
            if !ok {
                return;
            }
            while let Some(bytes) = output.recv().await {
                let alive = this
                    .update(cx, |this, cx| {
                        if let Some(t) = this.terminal.as_mut() {
                            t.parser.process(&bytes);
                        }
                        cx.notify();
                    })
                    .is_ok();
                if !alive {
                    return;
                }
            }
            let _ = this.update(cx, |this, cx| {
                if let Some(t) = this.terminal.as_mut() {
                    t.status = TermStatus::Closed;
                    t.input = None;
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn terminal_send(&mut self, bytes: Vec<u8>) {
        if let Some(tx) = self.terminal.as_ref().and_then(|t| t.input.as_ref()) {
            let _ = tx.send(ShellInput::Data(bytes));
        }
    }

    fn terminal_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let ks = &event.keystroke;
        let app_cursor = self.terminal.as_ref().is_some_and(|t| t.parser.screen().application_cursor());
        if ks.modifiers.platform && ks.key == "v" {
            if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                self.terminal_send(text.replace('\n', "\r").into_bytes());
            }
            return;
        }
        if let Some(bytes) = key_bytes(ks, app_cursor) {
            self.terminal_send(bytes);
        }
    }

    /// Opens the server in the macOS Terminal with the system ssh.
    pub(crate) fn open_external_terminal(&self, server: ServerId) {
        let Some(s) = self.servers.iter().find(|s| s.id == server) else { return };
        let mut cmd = format!("exec ssh -p {} ", s.port);
        if let sesame_core::AuthMethod::KeyFile { path } = &s.auth {
            cmd.push_str(&format!("-i '{}' ", path.replace('\'', "")));
        }
        cmd.push_str(&format!("'{}@{}'", s.user.replace('\'', ""), s.host.replace('\'', "")));
        let file = std::env::temp_dir().join(format!("sesame-{}.command", s.id));
        if std::fs::write(&file, format!("#!/bin/sh\n{cmd}\n")).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700));
            }
            let _ = std::process::Command::new("open").arg(&file).spawn();
        }
    }
}

impl SesameApp {
    /// Shows a terminal with sample output (screenshots, demos).
    #[doc(hidden)]
    pub fn demo_terminal(&mut self, server: ServerId, cx: &mut Context<Self>) {
        let mut parser = vt100::Parser::new(std::num::NonZeroU16::new(ROWS).unwrap(), std::num::NonZeroU16::new(COLS).unwrap(), 0);
        parser.process(b"Welcome to Ubuntu 24.04.4 LTS (GNU/Linux 6.8.0-138-generic aarch64)\r\n\r\n\x1b[1;32mroot@app-1\x1b[0m:\x1b[1;34m~\x1b[0m# systemctl status caddy --no-pager | head -3\r\n\x1b[1;32m\xe2\x97\x8f\x1b[0m caddy.service - Caddy\r\n     Loaded: loaded (/usr/lib/systemd/system/caddy.service; \x1b[1;32menabled\x1b[0m)\r\n     Active: \x1b[1;32mactive (running)\x1b[0m since Mon 2026-10-26 09:55:02 UTC\r\n\x1b[1;32mroot@app-1\x1b[0m:\x1b[1;34m~\x1b[0m# ls --color=always /home\r\n\x1b[1;34ms_lara_response_az\x1b[0m  \x1b[1;34ms_sesame_response_az\x1b[0m  \x1b[1;34mshop\x1b[0m\r\n\x1b[1;32mroot@app-1\x1b[0m:\x1b[1;34m~\x1b[0m# ");
        self.terminal = Some(TerminalState { server, parser, input: None, status: TermStatus::Open, error: None, focus: cx.focus_handle() });
        cx.notify();
    }
}

/// Bytes a key press sends to a terminal (xterm conventions).
pub fn key_bytes(ks: &Keystroke, app_cursor: bool) -> Option<Vec<u8>> {
    let m = &ks.modifiers;
    let arrow = |c: u8| if app_cursor { vec![0x1b, b'O', c] } else { vec![0x1b, b'[', c] };
    let bytes = match ks.key.as_str() {
        "enter" => vec![b'\r'],
        "backspace" => vec![0x7f],
        "tab" => if m.shift { b"\x1b[Z".to_vec() } else { vec![b'\t'] },
        "escape" => vec![0x1b],
        "up" => arrow(b'A'),
        "down" => arrow(b'B'),
        "right" => arrow(b'C'),
        "left" => arrow(b'D'),
        "home" => b"\x1b[H".to_vec(),
        "end" => b"\x1b[F".to_vec(),
        "pageup" => b"\x1b[5~".to_vec(),
        "pagedown" => b"\x1b[6~".to_vec(),
        "delete" => b"\x1b[3~".to_vec(),
        "space" if m.control => vec![0],
        key if m.control && key.len() == 1 => {
            let c = key.as_bytes()[0].to_ascii_lowercase();
            match c {
                b'a'..=b'z' => vec![c - b'a' + 1],
                b'[' => vec![0x1b],
                b'\\' => vec![0x1c],
                b']' => vec![0x1d],
                _ => return None,
            }
        }
        _ if m.platform => return None,
        _ => {
            let text = ks.key_char.clone().or_else(|| (ks.key == "space").then(|| " ".to_string()))?;
            let mut b = Vec::new();
            if m.alt {
                b.push(0x1b);
            }
            b.extend(text.into_bytes());
            b
        }
    };
    Some(bytes)
}

fn color(c: vt100::Color, fallback: Hsla, dark: bool) -> Hsla {
    const ANSI: [u32; 16] = [
        0x1d1b26, 0xe5534b, 0x57ab5a, 0xc69026, 0x539bf5, 0xb083f0, 0x39c5cf, 0xd1d7e0,
        0x636e7b, 0xff938a, 0x6bc46d, 0xdaaa3f, 0x6cb6ff, 0xdcbdfb, 0x56d4dd, 0xf0f6fc,
    ];
    match c {
        vt100::Color::Default => fallback,
        vt100::Color::Idx(i) if (i as usize) < 16 => {
            let mut v = ANSI[i as usize];
            if !dark && i == 7 {
                v = 0x57606a;
            }
            rgb(v).into()
        }
        vt100::Color::Idx(i) if i >= 232 => {
            let l = 8 + (i as u32 - 232) * 10;
            rgb((l << 16) | (l << 8) | l).into()
        }
        vt100::Color::Idx(i) => {
            let i = i as u32 - 16;
            let s = |v: u32| if v == 0 { 0 } else { 55 + v * 40 };
            rgb((s(i / 36) << 16) | (s((i / 6) % 6) << 8) | s(i % 6)).into()
        }
        vt100::Color::Rgb(r, g, b) => rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32).into(),
    }
}

pub fn view(app: &SesameApp, server: ServerId, cx: &mut Context<SesameApp>) -> Div {
    let g = Glass::get(cx).clone();
    let user = app.servers.iter().find(|s| s.id == server).map(|s| s.user.clone()).unwrap_or_default();
    let term = app.terminal.as_ref().filter(|t| t.server == server);
    let toolbar = h_flex()
        .gap_2()
        .child(div().flex_1().text_xs().text_color(g.faint).child(trp("term.lead", &[("user", &user)])))
        .child(
            Button::new("term-open")
                .primary()
                .icon(IconName::Terminal)
                .label(match term.map(|t| t.status) {
                    Some(TermStatus::Closed) => tr("term.reconnect"),
                    _ => tr("term.connect"),
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    if this.terminal.as_ref().is_some_and(|t| t.server == server && t.status == TermStatus::Closed) {
                        this.terminal = None;
                    }
                    this.open_terminal(server, window, cx);
                })),
        )
        .child(
            Button::new("term-external")
                .ghost()
                .icon(IconName::ExternalLink)
                .label(tr("term.external"))
                .on_click(cx.listener(move |this, _, _, _| this.open_external_terminal(server))),
        );

    let Some(t) = term else {
        return v_flex().gap_3().child(toolbar);
    };
    let screen = t.parser.screen();
    let (rows, cols) = screen.size();
    let (rows, cols): (u16, u16) = (rows.into(), cols.into());
    let (cur_r, cur_c) = screen.cursor_position();
    let fg_default = if g.dark { rgb(0xd8dee9).into() } else { rgb(0x24292f).into() };
    let bg_default = if g.dark { gpui_kit::hsla(250. / 360., 0.25, 0.06, 1.0) } else { rgb(0xfbfaf7).into() };
    let mut grid = v_flex().font_family("Menlo").text_size(px(13.)).line_height(px(CELL_H));
    for r in 0..rows {
        // One text element per line, coloured by ranges: the font decides
        // glyph widths, so columns line up exactly.
        let mut text = String::new();
        let mut highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
        for c in 0..cols {
            let cell = screen.cell(r, c);
            let (cell_text, fg, bg, bold) = match cell {
                Some(cell) if cell.is_wide_continuation() => continue,
                Some(cell) => {
                    let txt = if cell.has_contents() { cell.contents().to_string() } else { " ".to_string() };
                    let fg = color(cell.fgcolor(), fg_default, g.dark);
                    let bg = match cell.bgcolor() {
                        vt100::Color::Default => None,
                        other => Some(color(other, bg_default, g.dark)),
                    };
                    let (fg, bg) = if cell.inverse() { (bg.unwrap_or(bg_default), Some(fg)) } else { (fg, bg) };
                    (txt, fg, bg, cell.bold())
                }
                None => (" ".to_string(), fg_default, None, false),
            };
            let at_cursor = r == cur_r && c == cur_c && t.status == TermStatus::Open && !screen.hide_cursor();
            let (fg, bg) = if at_cursor { (bg_default, Some(g.accent)) } else { (fg, bg) };
            let start = text.len();
            text.push_str(&cell_text);
            let style = HighlightStyle {
                color: Some(fg),
                background_color: bg,
                font_weight: bold.then_some(FontWeight::BOLD),
                ..Default::default()
            };
            match highlights.last_mut() {
                Some((range, last)) if *last == style && range.end == start => range.end = text.len(),
                _ => highlights.push((start..text.len(), style)),
            }
        }
        grid = grid.child(div().h(px(CELL_H)).whitespace_nowrap().child(StyledText::new(text).with_highlights(highlights)));
    }
    let status_line = match (t.status, &t.error) {
        (_, Some(e)) => div().text_color(g.bad).child(e.clone()),
        (TermStatus::Connecting, _) => div().text_color(g.dim).child(tr("term.connecting")),
        (TermStatus::Closed, _) => div().text_color(g.dim).child(tr("term.closed")),
        (TermStatus::Open, _) => div(),
    };
    let focus = t.focus.clone();
    v_flex().gap_3().child(toolbar).child(status_line).child(
        div()
            .id("terminal")
            .track_focus(&focus)
            .key_context("Terminal")
            .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                this.terminal_key(e, cx);
                cx.stop_propagation();
            }))
            .on_click(cx.listener(move |_, _, window, cx| window.focus(&focus, cx)))
            .p_3()
            .rounded_lg()
            .border_1()
            .border_color(g.line)
            .bg(bg_default)
            .text_color(fg_default)
            .w_full()
            .overflow_hidden()
            .child(grid),
    )
}

#[cfg(test)]
mod tests {
    // Explicit imports: the GPUI glob would shadow `#[test]`.
    use super::key_bytes;
    use gpui_kit::{Keystroke, Modifiers};

    fn ks(key: &str, ctrl: bool, ch: Option<&str>) -> Keystroke {
        Keystroke {
            modifiers: Modifiers { control: ctrl, ..Default::default() },
            key: key.into(),
            key_char: ch.map(String::from),
        }
    }

    #[test]
    fn keys_map_to_xterm_bytes() {
        assert_eq!(key_bytes(&ks("c", true, None), false), Some(vec![3]));
        assert_eq!(key_bytes(&ks("enter", false, None), false), Some(vec![b'\r']));
        assert_eq!(key_bytes(&ks("up", false, None), false), Some(b"\x1b[A".to_vec()));
        assert_eq!(key_bytes(&ks("up", false, None), true), Some(b"\x1bOA".to_vec()));
        assert_eq!(key_bytes(&ks("a", false, Some("a")), false), Some(b"a".to_vec()));
        assert_eq!(key_bytes(&ks("backspace", false, None), false), Some(vec![0x7f]));
    }
}
