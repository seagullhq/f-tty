//! Jump between shell prompts (OSC 133;A marks), Ctrl+Shift+Up / Down.

use crate::app::TerminalView;
use crate::terminal::Term;
use gpui::*;

/// Buffer lines that start a prompt, oldest first.
fn prompt_lines(term: &Term) -> Vec<usize> {
    (0..term.history_len() + term.rows)
        .filter(|&i| term.line(i).iter().any(|c| c.mark))
        .collect()
}

/// Nearest prompt line above (`up`) or below `top`, if any.
fn target(lines: &[usize], top: usize, up: bool) -> Option<usize> {
    if up {
        lines.iter().rev().find(|&&l| l < top).copied()
    } else {
        lines.iter().find(|&&l| l > top).copied()
    }
}

impl TerminalView {
    /// Puts the previous / next prompt at the top of the view; past the last
    /// one, returns to the present.
    pub(crate) fn jump_prompt(&mut self, up: bool, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let term = &tab.emu.term;
        let history = term.history_len();
        let top = term.visible_top(tab.scroll);
        let scroll = match target(&prompt_lines(term), top, up) {
            Some(line) => history - line.min(history),
            None if up => return,
            None => 0,
        };
        if scroll != tab.scroll {
            tab.scroll = scroll;
            tab.scroll_acc = 0.;
            self.selection = None;
            self.selecting = false;
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn marks_survive_scrolling_and_drive_jumps() {
        let mut emu = crate::terminal::Emulator::new(10, 3);
        emu.feed(b"\x1b]133;A\x07$ a\r\nout\r\n\x1b]133;A\x07$ b\r\nout\r\n\x1b]133;A\x07$ c");
        let lines = prompt_lines(&emu.term);
        assert_eq!(lines.len(), 3);
        assert_eq!(target(&lines, lines[2], true), Some(lines[1]));
        assert_eq!(target(&lines, lines[0], true), None);
        assert_eq!(target(&lines, lines[1], false), Some(lines[2]));
        assert_eq!(target(&lines, lines[2], false), None);
    }
}
