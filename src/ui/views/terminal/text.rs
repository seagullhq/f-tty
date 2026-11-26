//! Grid text: turns one row of terminal cells into a styled line.

use crate::terminal::{Cell, DEFAULT_BG, DEFAULT_FG, Style};
use crate::ui::theme::color_of;
use gpui::*;

#[derive(Clone, Copy, PartialEq, Eq)]
struct RunKey {
    fg: u32,
    bg: u32,
    bold: bool,
    link: bool,
}

/// Length to draw: trailing unstyled spaces should not be laid out or
/// painted, but the cursor must stay visible even on an empty cell.
/// History lines are stored without their trailing blanks.
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
    len.max(cursor_col.map_or(0, |col| col + 1))
}

/// Builds a row as a single text with contiguous style runs.
pub(crate) fn line_from_cells(
    cells: &[Cell],
    cursor_col: Option<usize>,
    family: &SharedString,
) -> Div {
    let len = line_len(cells, cursor_col);

    let regular = font(family.clone());
    let bold = Font {
        weight: FontWeight::BOLD,
        ..font(family.clone())
    };

    let mut text = String::with_capacity(len);
    let mut runs: Vec<TextRun> = Vec::new();
    let mut last: Option<RunKey> = None;

    for col in 0..len {
        let cell = cells.get(col).copied().unwrap_or_default();
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
            link: cell.link != 0,
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
                underline: key.link.then(|| UnderlineStyle {
                    thickness: px(1.),
                    color: None,
                    wavy: false,
                }),
                strikethrough: None,
            });
            last = Some(key);
        }
        text.push(cell.ch);
    }

    div().child(StyledText::new(text).with_runs(runs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::Color;

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
