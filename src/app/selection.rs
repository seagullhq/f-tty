//! Scrollback navigation and text selection.

use crate::app::TerminalView;
use crate::terminal::Term;
use crate::ui::theme::{SELECTION, color_of};
use gpui::*;

impl TerminalView {
    /// Positive lines = go back in history, negative = return to the present.
    pub(crate) fn scroll_active(&mut self, lines: f32, cx: &mut Context<Self>) {
        if self.settings_open {
            return;
        }
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let target = apply_scroll(
            tab.scroll,
            tab.emu.term.history_len(),
            lines,
            &mut tab.scroll_acc,
        );
        if target != tab.scroll {
            tab.scroll = target;
            self.selection = None;
            self.selecting = false;
            cx.notify();
        }
    }

    pub(crate) fn scroll_page(&mut self, up: bool, cx: &mut Context<Self>) {
        let page = self
            .tabs
            .get(self.active)
            .map(|tab| tab.emu.term.rows.saturating_sub(1).max(1))
            .unwrap_or(1) as f32;
        self.scroll_active(if up { page } else { -page }, cx);
    }

    pub(crate) fn cell_at(&self, position: Point<Pixels>) -> (usize, usize) {
        let term = &self.tabs[self.active].emu.term;
        let origin = self.text_origin();
        let col = ((position.x - origin.x) / self.cell_width).floor();
        let row = ((position.y - origin.y) / self.line_height).floor();
        (
            (row.max(0.) as usize).min(term.rows.saturating_sub(1)),
            (col.max(0.) as usize).min(term.cols.saturating_sub(1)),
        )
    }

    /// Linear range [start, end) of the selection.
    fn selection_range(&self) -> Option<(usize, usize)> {
        let selection = self.selection?;
        let cols = self.tabs[self.active].emu.term.cols;
        let a = selection.anchor.0 * cols + selection.anchor.1;
        let b = selection.cursor.0 * cols + selection.cursor.1;
        Some((a.min(b), a.max(b) + 1))
    }

    /// Highlights the selection with translucent rectangles above the text:
    /// text layout does not change, so dragging stays smooth.
    pub(crate) fn selection_overlays(&self, term: &Term) -> Vec<Div> {
        let Some((start, end)) = self.selection_range() else {
            return Vec::new();
        };
        if term.cols == 0 || start >= end {
            return Vec::new();
        }
        let color = color_of(SELECTION).opacity(0.4);
        let mut rects = Vec::new();
        let first_row = start / term.cols;
        let last_row = (end - 1) / term.cols;
        for row in first_row..=last_row {
            let row_start = start.max(row * term.cols);
            let row_end = end.min((row + 1) * term.cols);
            if row_start >= row_end {
                continue;
            }
            let col = row_start - row * term.cols;
            let cells = row_end - row_start;
            rects.push(
                div()
                    .absolute()
                    .left(self.cell_width * col as f32)
                    .top(self.line_height * row as f32)
                    .w(self.cell_width * cells as f32)
                    .h(self.line_height)
                    .bg(color),
            );
        }
        rects
    }

    pub(crate) fn copy_selection(&self, cx: &mut Context<Self>) {
        let Some(selection) = self.selection else {
            return;
        };
        let term = &self.tabs[self.active].emu.term;
        let text = selection_text(term, selection.anchor, selection.cursor);
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }
}

/// Applies a delta in lines (positive = back), keeping touchpad fractional
/// remainders; the result is always within the history.
fn apply_scroll(scroll: usize, history: usize, delta: f32, acc: &mut f32) -> usize {
    let total = *acc + delta;
    let whole = total.trunc();
    *acc = total - whole;
    (scroll as f32 + whole).clamp(0., history as f32) as usize
}

/// Selected text between two cells (inclusive), rows joined by newline and
/// trailing spaces trimmed like terminals do.
fn selection_text(term: &Term, anchor: (usize, usize), cursor: (usize, usize)) -> String {
    if term.cols == 0 || term.rows == 0 {
        return String::new();
    }
    let (start, end) = if anchor <= cursor {
        (anchor, cursor)
    } else {
        (cursor, anchor)
    };
    let start_index = start.0 * term.cols + start.1;
    let end_index = (end.0 * term.cols + end.1).min((term.rows * term.cols).saturating_sub(1));
    if start_index > end_index {
        return String::new();
    }
    let mut text = String::new();
    let mut row = start.0;
    let mut line = String::new();
    for index in start_index..=end_index {
        let r = index / term.cols;
        if r != row {
            text.push_str(line.trim_end());
            text.push('\n');
            line.clear();
            row = r;
        }
        line.push(term.row(index / term.cols)[index % term.cols].ch);
    }
    text.push_str(line.trim_end());
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn scroll_offset_clamps_and_accumulates() {
        let mut acc = 0.;
        assert_eq!(apply_scroll(0, 10, 2., &mut acc), 2);
        assert_eq!(apply_scroll(2, 10, -5., &mut acc), 0);
        assert_eq!(apply_scroll(0, 10, 1.5, &mut acc), 1);
        assert_eq!(apply_scroll(1, 10, 0.6, &mut acc), 2);
        assert_eq!(apply_scroll(2, 10, 100., &mut acc), 10);
    }

    #[::core::prelude::v1::test]
    fn selection_text_joins_rows() {
        let mut emu = crate::terminal::Emulator::new(4, 3);
        emu.feed(b"abcd\r\nefgh\r\nijkl");
        let term = &emu.term;
        assert_eq!(selection_text(term, (0, 1), (1, 2)), "bcd\nefg");
        assert_eq!(selection_text(term, (2, 2), (1, 1)), "fgh\nijk");
    }
}
