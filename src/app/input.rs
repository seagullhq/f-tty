//! Keyboard and mouse event handling.

use crate::app::tree::Dir;
use crate::app::{Selection, TerminalView};
use gpui::*;
use std::time::Instant;

impl TerminalView {
    pub(crate) fn on_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ks = &ev.keystroke;

        // Tab shortcuts (not sent to the shell).
        if ks.modifiers.control {
            if ks.modifiers.shift && ks.modifiers.alt {
                // Pane layout: resize, equalize, swap.
                if let Some(dir) = Dir::parse(&ks.key) {
                    return self.resize_pane_key(dir, cx);
                }
                match ks.key.as_str() {
                    "=" | "+" => return self.equalize_panes(cx),
                    "[" | "{" => return self.swap_pane(-1, cx),
                    "]" | "}" => return self.swap_pane(1, cx),
                    _ => {}
                }
            } else if ks.modifiers.shift {
                if let Some(dir) = Dir::parse(&ks.key) {
                    return self.focus_direction(dir, cx);
                }
                match ks.key.as_str() {
                    "c" => {
                        self.copy_selection(cx);
                        return;
                    }
                    "f" => return self.open_search(cx),
                    "v" => return self.paste(cx),
                    "t" => return self.open_tab(cx),
                    "w" => {
                        if self.settings_open {
                            self.settings_open = false;
                            cx.notify();
                        } else {
                            self.close_pane(window, cx);
                        }
                        return;
                    }
                    "d" => return self.split_pane(Axis::Horizontal, cx),
                    "e" => return self.split_pane(Axis::Vertical, cx),
                    "[" | "{" => return self.cycle_pane(-1, cx),
                    "]" | "}" => return self.cycle_pane(1, cx),
                    "z" | "enter" => return self.toggle_zoom(cx),
                    "b" => return self.toggle_broadcast(cx),
                    "tab" => return self.cycle_tab(-1, cx),
                    "pageup" => return self.jump_prompt(true, cx),
                    "pagedown" => return self.jump_prompt(false, cx),
                    _ => {}
                }
            } else if ks.key == "tab" {
                return self.cycle_tab(1, cx);
            }
        }

        if self.search.is_some() && !self.settings_open {
            return self.search_key(ks, cx);
        }

        if self.settings_open {
            if self.font_picker.is_open() {
                if let Some(family) = self.font_picker.on_key(ks, &self.fonts, cx) {
                    self.select_font(family, window, cx);
                }
                if !self.font_picker.is_open() {
                    window.focus(&self.focus, cx);
                }
                cx.notify();
            }
            return;
        }

        if ks.modifiers.shift && matches!(ks.key.as_str(), "pageup" | "pagedown") {
            self.scroll_page(ks.key == "pageup", cx);
            return;
        }

        if ks.modifiers.shift && ks.key == "insert" {
            return self.paste(cx);
        }

        if let Some(mut bytes) = Self::keystroke_bytes(ks) {
            // DECCKM: arrows and Home/End use SS3 instead of CSI.
            let app_cursor = self
                .tabs
                .get(self.active)
                .is_some_and(|t| t.emu.term.app_cursor);
            if app_cursor
                && bytes.len() == 3
                && bytes[..2] == *b"\x1b["
                && b"ABCDHF".contains(&bytes[2])
            {
                bytes[1] = b'O';
            }
            if let Some(tab) = self.tabs.get_mut(self.active) {
                // Typing always jumps back to the present.
                tab.scroll = 0;
                tab.scroll_acc = 0.;
                // Enter submits the line (the shell draws a fresh prompt);
                // Ctrl+C cancels it.
                if bytes == b"\r" || bytes == b"\x03" {
                    tab.busy_since = (bytes == b"\r").then(Instant::now);
                }
            }
            self.send(&bytes);
        }
    }

    /// Text produced by a keystroke. GPUI reports the spacebar as `"space"`
    /// on all platforms, but on Windows `key_char` is `None` for it.
    pub(crate) fn typed_text(ks: &Keystroke) -> Option<String> {
        match ks.key.as_str() {
            "space" | " " => Some(" ".to_string()),
            _ => ks.key_char.clone(),
        }
    }

    /// Encodes a keystroke as terminal input (xterm-style escape sequences).
    fn keystroke_bytes(ks: &Keystroke) -> Option<Vec<u8>> {
        let m = &ks.modifiers;
        match ks.key.as_str() {
            "enter" => Some(b"\r".to_vec()),
            "backspace" => Some(vec![0x7f]),
            "tab" => Some(b"\t".to_vec()),
            "escape" => Some(vec![0x1b]),
            "up" | "arrowup" => Some(b"\x1b[A".to_vec()),
            "down" | "arrowdown" => Some(b"\x1b[B".to_vec()),
            "right" => Some(b"\x1b[C".to_vec()),
            "left" => Some(b"\x1b[D".to_vec()),
            "home" => Some(b"\x1b[H".to_vec()),
            "end" => Some(b"\x1b[F".to_vec()),
            "insert" => Some(b"\x1b[2~".to_vec()),
            "delete" => Some(b"\x1b[3~".to_vec()),
            "pageup" => Some(b"\x1b[5~".to_vec()),
            "pagedown" => Some(b"\x1b[6~".to_vec()),
            "f1" => Some(b"\x1bOP".to_vec()),
            "f2" => Some(b"\x1bOQ".to_vec()),
            "f3" => Some(b"\x1bOR".to_vec()),
            "f4" => Some(b"\x1bOS".to_vec()),
            "f5" => Some(b"\x1b[15~".to_vec()),
            "f6" => Some(b"\x1b[17~".to_vec()),
            "f7" => Some(b"\x1b[18~".to_vec()),
            "f8" => Some(b"\x1b[19~".to_vec()),
            "f9" => Some(b"\x1b[20~".to_vec()),
            "f10" => Some(b"\x1b[21~".to_vec()),
            "f11" => Some(b"\x1b[23~".to_vec()),
            "f12" => Some(b"\x1b[24~".to_vec()),
            // `key_char` is empty for the spacebar on Windows: use the key name.
            "space" | " " if m.control => Some(vec![0x00]),
            "space" | " " => Some(b" ".to_vec()),
            // AltGr can be reported as Ctrl+Alt; in that case key_char holds the
            // real character (e.g. '@') and must win over the control byte.
            key if m.control && !m.alt && key.is_ascii() && key.len() == 1 => {
                Some(vec![key.as_bytes()[0].to_ascii_lowercase() & 0x1f])
            }
            _ => {
                let text = TerminalView::typed_text(ks)?;
                let mut bytes = Vec::with_capacity(text.len() + 1);
                // Alt acts as Meta in a terminal.
                if m.alt && !m.control && !m.platform {
                    bytes.push(0x1b);
                }
                bytes.extend_from_slice(text.as_bytes());
                Some(bytes)
            }
        }
    }

    /// Writes to the focused pane, or to every pane while broadcasting.
    fn send(&mut self, bytes: &[u8]) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if tab.broadcast {
            for pane in &mut tab.panes {
                if let Some(pty) = &mut pane.pty {
                    let _ = pty.write(bytes);
                }
            }
        } else if let Some(pty) = &mut tab.pty {
            let _ = pty.write(bytes);
        }
    }

    /// Pastes the clipboard, wrapped in bracketed-paste markers when the
    /// program asked for them.
    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        tab.scroll = 0;
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        let bytes = if tab.emu.term.bracketed_paste {
            // Strip a pasted end marker so it cannot close the paste early.
            format!("\x1b[200~{}\x1b[201~", text.replace("\x1b[201~", "")).into_bytes()
        } else {
            text.into_bytes()
        };
        self.send(&bytes);
        cx.notify();
    }

    /// Reports a mouse event to the program when it enabled mouse tracking
    /// (and Shift is not held to force a local selection).
    /// `button`: 0 left, 3 release (legacy), 32+ motion, 64/65 wheel.
    fn report_mouse(&mut self, button: u8, cell: (usize, usize), press: bool, shift: bool) -> bool {
        let Some(term) = self.tabs.get(self.active).map(|t| &t.emu.term) else {
            return false;
        };
        if term.mouse == 0 || shift {
            return false;
        }
        let (x, y) = (cell.1 + 1, cell.0 + 1);
        let bytes = if term.mouse_sgr {
            format!("\x1b[<{button};{x};{y}{}", if press { 'M' } else { 'm' }).into_bytes()
        } else {
            let b = if press { button } else { 3 };
            vec![
                0x1b,
                b'[',
                b'M',
                32 + b,
                (32 + x).min(255) as u8,
                (32 + y).min(255) as u8,
            ]
        };
        self.send(&bytes);
        true
    }

    pub(crate) fn on_terminal_scroll(
        &mut self,
        ev: &ScrollEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let lines = match ev.delta {
            ScrollDelta::Lines(delta) => delta.y,
            ScrollDelta::Pixels(delta) => delta.y / self.line_height,
        };
        let cell = self.cell_at(ev.position);
        let steps = lines.round() as i32;
        if !self.settings_open && steps != 0 {
            let up = steps > 0;
            if self.report_mouse(if up { 64 } else { 65 }, cell, true, ev.modifiers.shift) {
                return;
            }
            // Alternate scroll: less/man/vim have no scrollback, so the
            // wheel becomes arrow keys.
            let term = &self.tabs[self.active].emu.term;
            if term.in_alt_screen() {
                let mid = if term.app_cursor { b'O' } else { b'[' };
                let key = if up { b'A' } else { b'B' };
                let seq = [0x1b, mid, key].repeat(steps.unsigned_abs() as usize);
                self.send(&seq);
                return;
            }
        }
        self.scroll_active(lines, cx);
    }

    pub(crate) fn on_terminal_mouse_down(
        &mut self,
        ev: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.settings_open && self.on_scrollbar(ev.position) {
            self.scrollbar_drag = true;
            self.scrollbar_to(ev.position, cx);
            return;
        }
        if self.settings_open || self.tabs[self.active].scroll > 0 {
            return;
        }
        if ev.modifiers.control {
            if let Some(uri) = self.link_at(ev.position) {
                cx.open_url(&uri);
                return;
            }
        }
        let cell = self.cell_at(ev.position);
        if self.report_mouse(0, cell, true, ev.modifiers.shift) {
            self.mouse_reported = true;
            return;
        }
        self.selection = Some(Selection {
            anchor: cell,
            cursor: cell,
        });
        self.selecting = true;
        cx.notify();
    }

    pub(crate) fn on_terminal_mouse_move(
        &mut self,
        ev: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.split_drag.is_some() {
            return self.drag_divider(ev, cx);
        }
        if self.scrollbar_drag {
            if ev.pressed_button == Some(MouseButton::Left) {
                self.scrollbar_to(ev.position, cx);
            } else {
                self.scrollbar_drag = false;
                cx.notify();
            }
            return;
        }
        if self.mouse_reported {
            let mode = self.tabs[self.active].emu.term.mouse;
            // 1002 reports drags, 1003 every move.
            if mode != 1000 {
                let cell = self.cell_at(ev.position);
                self.report_mouse(32, cell, true, false);
            }
            return;
        }
        if !self.selecting || ev.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let cell = self.cell_at(ev.position);
        if let Some(selection) = &mut self.selection {
            if selection.cursor != cell {
                selection.cursor = cell;
                cx.notify();
            }
        }
    }

    pub(crate) fn on_terminal_mouse_up(
        &mut self,
        ev: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.split_drag.take().is_some() {
            return;
        }
        if std::mem::take(&mut self.scrollbar_drag) {
            cx.notify();
            return;
        }
        if std::mem::take(&mut self.mouse_reported) {
            let cell = self.cell_at(ev.position);
            self.report_mouse(0, cell, false, false);
            return;
        }
        if !self.selecting {
            return;
        }
        self.selecting = false;
        if self
            .selection
            .is_some_and(|selection| selection.anchor == selection.cursor)
        {
            self.selection = None;
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(key: &str, key_char: Option<&str>) -> Keystroke {
        Keystroke {
            modifiers: Modifiers::none(),
            key: key.to_string(),
            key_char: key_char.map(str::to_string),
        }
    }

    #[::core::prelude::v1::test]
    fn space_works_when_gpui_leaves_key_char_empty() {
        // This is what Windows sends: key = "space", key_char = None.
        assert_eq!(
            TerminalView::keystroke_bytes(&stroke("space", None)),
            Some(b" ".to_vec())
        );
        assert_eq!(
            TerminalView::keystroke_bytes(&stroke(" ", Some(" "))),
            Some(b" ".to_vec())
        );
        assert_eq!(
            TerminalView::typed_text(&stroke("space", None)).as_deref(),
            Some(" ")
        );
    }

    #[::core::prelude::v1::test]
    fn special_keys_use_terminal_escape_sequences() {
        let expected = [
            ("insert", b"\x1b[2~".to_vec()),
            ("delete", b"\x1b[3~".to_vec()),
            ("pageup", b"\x1b[5~".to_vec()),
            ("pagedown", b"\x1b[6~".to_vec()),
            ("f1", b"\x1bOP".to_vec()),
            ("f5", b"\x1b[15~".to_vec()),
            ("f12", b"\x1b[24~".to_vec()),
        ];
        for (key, bytes) in expected {
            assert_eq!(
                TerminalView::keystroke_bytes(&stroke(key, None)),
                Some(bytes),
                "key {key}"
            );
        }
    }

    #[::core::prelude::v1::test]
    fn control_and_alt_modifiers_are_encoded() {
        let mut ctrl = stroke("a", None);
        ctrl.modifiers.control = true;
        assert_eq!(TerminalView::keystroke_bytes(&ctrl), Some(vec![0x01]));

        let mut ctrl_space = stroke("space", None);
        ctrl_space.modifiers.control = true;
        assert_eq!(TerminalView::keystroke_bytes(&ctrl_space), Some(vec![0x00]));

        let mut alt = stroke("a", Some("a"));
        alt.modifiers.alt = true;
        assert_eq!(TerminalView::keystroke_bytes(&alt), Some(vec![0x1b, b'a']));

        // AltGr may arrive as Ctrl+Alt: send the character, not Ctrl+2.
        let mut altgr = stroke("2", Some("@"));
        altgr.modifiers.control = true;
        altgr.modifiers.alt = true;
        assert_eq!(TerminalView::keystroke_bytes(&altgr), Some(b"@".to_vec()));
    }
}
