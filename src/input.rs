//! Keyboard and mouse event handling.

use crate::app::{Selection, TerminalView};
use gpui::*;

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
            if ks.modifiers.shift {
                match ks.key.as_str() {
                    "c" => {
                        self.copy_selection(cx);
                        return;
                    }
                    "t" => return self.open_tab(cx),
                    "w" => {
                        if self.settings_open {
                            self.settings_open = false;
                            cx.notify();
                        } else {
                            self.close_tab(self.active, window, cx);
                        }
                        return;
                    }
                    "tab" => return self.cycle_tab(-1, cx),
                    _ => {}
                }
            } else if ks.key == "tab" {
                return self.cycle_tab(1, cx);
            }
        }

        if self.settings_open {
            if self.font_picker_open {
                self.on_font_search_key(ks, window, cx);
            }
            return;
        }

        if ks.modifiers.shift && matches!(ks.key.as_str(), "pageup" | "pagedown") {
            self.scroll_page(ks.key == "pageup", cx);
            return;
        }

        if let Some(bytes) = Self::keystroke_bytes(ks) {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                // Typing always jumps back to the present.
                tab.scroll = 0;
                tab.scroll_acc = 0.;
                // Enter submits the line (the shell draws a fresh prompt);
                // Ctrl+C cancels it. Any other key leaves input pending.
                if bytes == b"\r" || bytes == b"\x03" {
                    tab.input_pending = false;
                } else {
                    tab.input_pending = true;
                }
                if let Some(pty) = &mut tab.pty {
                    let _ = pty.write(&bytes);
                }
            }
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

    pub(crate) fn on_terminal_scroll(
        &mut self,
        ev: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let lines = match ev.delta {
            ScrollDelta::Lines(delta) => delta.y,
            ScrollDelta::Pixels(delta) => delta.y / self.line_height,
        };
        self.scroll_active(lines, cx);
    }

    pub(crate) fn on_terminal_mouse_down(
        &mut self,
        ev: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_open || self.tabs[self.active].scroll > 0 {
            return;
        }
        let cell = self.cell_at(ev.position);
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
        _ev: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
