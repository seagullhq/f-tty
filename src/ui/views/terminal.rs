//! Rendering of the terminal grid.

use crate::app::TerminalView;
use crate::terminal::{Cell, DEFAULT_BG, DEFAULT_FG, Style};
use crate::ui::theme::{self, color_of};
use gpui::prelude::FluentBuilder;
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

pub(crate) fn terminal_screen(view: &TerminalView, cx: &mut Context<TerminalView>) -> Div {
    let tab = &view.tabs[view.active];
    let split = tab.panes.len() > 1;
    let (base_x, base_y) = (px(tab.rect.0), px(tab.rect.1));
    let mut children: Vec<AnyElement> = Vec::new();
    for (j, pane) in tab.panes.iter().enumerate() {
        if tab.zoomed && j != tab.focus {
            continue;
        }
        let focused = j == tab.focus;
        let term = &pane.emu.term;
        let top = term.visible_top(pane.scroll);
        let cursor = (focused && pane.scroll == 0 && term.cursor_visible)
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
        // Tags in the corner: what is special about this tab right now.
        let tags: Vec<&str> = [(tab.zoomed, "zoomed"), (tab.broadcast, "broadcast")]
            .into_iter()
            .filter_map(|(on, name)| on.then_some(name))
            .collect();

        children.push(
            div()
                .id(("pane", j))
                .absolute()
                .left(pane.origin.x - base_x)
                .top(pane.origin.y - base_y)
                .w(pane.area.width)
                .h(pane.area.height)
                .overflow_hidden()
                .bg(color_of(DEFAULT_BG))
                .cursor_text()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                        this.focus_pane(j, cx);
                        this.on_terminal_mouse_down(ev, window, cx);
                    }),
                )
                .on_scroll(cx.listener(move |this, ev: &ScrollEvent, window, cx| {
                    this.focus_pane(j, cx);
                    this.on_terminal_scroll(ev, window, cx);
                }))
                .children(lines)
                .when(focused, |d| {
                    d.children(view.selection_overlays(term))
                        .children(view.search_overlays(term, top))
                        .children(view.search_bar())
                        .children(view.scrollbar())
                })
                // Unfocused panes fade, as in iTerm2, WezTerm, Ghostty and Warp.
                .when(split && !focused, |d| {
                    d.child(
                        div()
                            .absolute()
                            .size_full()
                            .bg(color_of(DEFAULT_BG).opacity(0.4)),
                    )
                })
                // Marks the focused pane with a thin bar on its top edge (red
                // while broadcasting); absolute, so it never shifts the text.
                .when(split && focused, |d| {
                    d.child(
                        div()
                            .absolute()
                            .top(px(0.))
                            .left(px(0.))
                            .w_full()
                            .h(px(2.))
                            .bg(color_of(if tab.broadcast {
                                theme::DANGER
                            } else {
                                theme::ACCENT
                            })),
                    )
                })
                .when(focused && !tags.is_empty(), |d| {
                    d.child(
                        div()
                            .absolute()
                            .top(px(6.))
                            .left(px(6.))
                            .px(px(6.))
                            .rounded(theme::CONTROL_RADIUS)
                            .bg(color_of(theme::BADGE_BG))
                            .text_xs()
                            .text_color(color_of(theme::MUTED))
                            .child(tags.join(" · ")),
                    )
                })
                .into_any_element(),
        );
    }

    // Grab strips between panes: drag to resize, double click to equalize.
    if !tab.zoomed {
        for (k, d) in tab
            .tree
            .layout(tab.rect, f32::from(theme::SPLIT_GAP))
            .dividers
            .into_iter()
            .enumerate()
        {
            let (x, y, w, h) = d.rect;
            let (path, index) = (d.path, d.index);
            let strip = div()
                .id(("divider", k))
                .absolute()
                .left(px(x) - base_x)
                .top(px(y) - base_y)
                .w(px(w))
                .h(px(h))
                .hover(|s| s.bg(color_of(theme::ACCENT).opacity(0.4)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        this.start_divider_drag(path.clone(), index, ev, cx);
                    }),
                );
            children.push(
                if d.axis == Axis::Horizontal {
                    strip.cursor_col_resize()
                } else {
                    strip.cursor_row_resize()
                }
                .into_any_element(),
            );
        }
    }

    // Clip is inside the padding: even when shrunk, the card keeps its
    // border and bottom margin visible. Dividers show the container color.
    div()
        .relative()
        .flex_1()
        .w_full()
        .min_w(px(0.))
        .min_h(px(0.))
        .overflow_hidden()
        .bg(color_of(theme::BORDER))
        .font_family(view.font.clone())
        .text_sm()
        .on_mouse_move(cx.listener(TerminalView::on_terminal_mouse_move))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(TerminalView::on_terminal_mouse_up),
        )
        .children(children)
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
