//! Rendering of the terminal grid.

use crate::app::TerminalView;
use crate::term::{Cell, DEFAULT_BG, DEFAULT_FG, Style};
use crate::theme::color_of;
use gpui::*;

#[derive(Clone, Copy, PartialEq, Eq)]
struct RunKey {
    fg: u32,
    bg: u32,
    bold: bool,
}

/// Length to draw: trailing unstyled spaces should not be laid out or
/// painted, but the cursor must stay visible even on an empty cell.
fn line_len(cells: &[Cell], cursor_col: Option<usize>) -> usize {
    let mut len = cells.len();
    while len > 0 {
        let cell = &cells[len - 1];
        if cell.ch == ' ' && cell.style == Style::default() && cursor_col != Some(len - 1) {
            len -= 1;
        } else {
            break;
        }
    }
    len
}

/// Builds a row as a single text with contiguous style runs.
fn line_from_cells(cells: &[Cell], cursor_col: Option<usize>, family: &SharedString) -> Div {
    let len = line_len(cells, cursor_col);

    let regular = font(family.clone());
    let bold = Font {
        weight: FontWeight::BOLD,
        ..font(family.clone())
    };

    let mut text = String::with_capacity(len);
    let mut runs: Vec<TextRun> = Vec::new();
    let mut last: Option<RunKey> = None;

    for (col, cell) in cells[..len].iter().enumerate() {
        let is_cursor = cursor_col == Some(col);
        let mut fg = cell.style.fg.resolve(DEFAULT_FG);
        let mut bg = cell.style.bg.resolve(DEFAULT_BG);
        if cell.style.inverse ^ is_cursor {
            std::mem::swap(&mut fg, &mut bg);
        }

        let key = RunKey {
            fg,
            bg,
            bold: cell.style.bold,
        };

        if last == Some(key) {
            if let Some(run) = runs.last_mut() {
                run.len += cell.ch.len_utf8();
            }
        } else {
            runs.push(TextRun {
                len: cell.ch.len_utf8(),
                font: if key.bold {
                    bold.clone()
                } else {
                    regular.clone()
                },
                color: color_of(fg),
                background_color: (bg != DEFAULT_BG).then(|| color_of(bg)),
                underline: None,
                strikethrough: None,
            });
            last = Some(key);
        }
        text.push(cell.ch);
    }

    div().child(StyledText::new(text).with_runs(runs))
}

pub(crate) fn terminal_screen(view: &TerminalView, cx: &mut Context<TerminalView>) -> Div {
    let tab = &view.tabs[view.active];
    let term = &tab.emu.term;
    let top = term.visible_top(tab.scroll);
    let cursor = (tab.scroll == 0 && term.cursor_visible)
        .then(|| (term.history_len() + term.cursor.0, term.cursor.1));
    let lines: Vec<Div> = (0..term.rows)
        .map(|r| {
            let index = top + r;
            let cursor_col = match cursor {
                Some((cr, cc)) if cr == index => Some(cc),
                _ => None,
            };
            line_from_cells(term.line(index), cursor_col, &view.font)
        })
        .collect();

    // Clip is inside the padding: even when shrunk, the card
    // keeps its border and bottom margin visible.
    div()
        .relative()
        .flex_1()
        .w_full()
        .min_w(px(0.))
        .min_h(px(0.))
        .overflow_hidden()
        .cursor_text()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(TerminalView::on_terminal_mouse_down),
        )
        .on_mouse_move(cx.listener(TerminalView::on_terminal_mouse_move))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(TerminalView::on_terminal_mouse_up),
        )
        .on_scroll_wheel(cx.listener(TerminalView::on_terminal_scroll))
        .font_family(view.font.clone())
        .text_sm()
        .children(lines)
        .children(view.selection_overlays(term))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::term::Color;

    fn cells(text: &str, width: usize) -> Vec<Cell> {
        let mut cells = vec![Cell::default(); width];
        for (cell, ch) in cells.iter_mut().zip(text.chars()) {
            cell.ch = ch;
        }
        cells
    }

    #[::core::prelude::v1::test]
    fn line_len_trims_only_trailing_default_cells() {
        assert_eq!(line_len(&cells("", 8), None), 0);
        assert_eq!(line_len(&cells("ab", 8), None), 2);

        // A cell with a colored background is not "empty" and stays visible.
        let mut c = cells("ab", 8);
        c[5].style.bg = Color::Indexed(1);
        assert_eq!(line_len(&c, None), 6);

        // The cursor past the text is not trimmed.
        assert_eq!(line_len(&c, Some(7)), 8);
    }
}
