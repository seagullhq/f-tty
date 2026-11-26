//! Scrollback search: Ctrl+Shift+F opens a bar, Enter / Shift+Enter step
//! through matches, Esc closes it.

use crate::app::TerminalView;
use crate::terminal::{Cell, Term};
use crate::ui::components::{TextInput, icon};
use crate::ui::theme::{self, color_of};
use gpui::*;

pub(crate) struct Search {
    /// The text field the user types into.
    pub(crate) input: Entity<TextInput>,
    /// Copy of the field's text, the one matches are computed from.
    pub(crate) query: String,
    /// Keeps the field observed while the bar is open.
    _watch: Subscription,
    /// Index of the current match among all matches, and its position
    /// (buffer line, column).
    pub(crate) index: usize,
    pub(crate) pos: Option<(usize, usize)>,
    pub(crate) total: usize,
}

/// Columns where `needle` (already lowercase) starts in `cells`,
/// case-insensitive. Cells are one char each, so a column is a char index.
pub(crate) fn line_hits(cells: &[Cell], needle: &[char]) -> Vec<usize> {
    if needle.is_empty() || cells.len() < needle.len() {
        return Vec::new();
    }
    let hay: Vec<char> = cells
        .iter()
        .map(|c| c.ch.to_lowercase().next().unwrap_or(c.ch))
        .collect();
    (0..=hay.len() - needle.len())
        .filter(|&i| hay[i..i + needle.len()] == *needle)
        .collect()
}

fn needle(query: &str) -> Vec<char> {
    query
        .chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect()
}

/// Every match in the buffer, oldest first.
fn all_hits(term: &Term, query: &str) -> Vec<(usize, usize)> {
    let needle = needle(query);
    (0..term.history_len() + term.rows)
        .flat_map(|line| {
            line_hits(term.line(line), &needle)
                .into_iter()
                .map(move |col| (line, col))
        })
        .collect()
}

impl TerminalView {
    pub(crate) fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.is_some() || self.settings_open {
            return;
        }
        let input = cx.new(|cx| TextInput::new("Find in scrollback", cx));
        let handle = input.read(cx).focus_handle();
        let watch = cx.observe(&input, |this, input, cx| {
            let text = input.read(cx).text().to_string();
            this.search_sync(text, cx);
        });
        self.search = Some(Search {
            input,
            query: String::new(),
            _watch: watch,
            index: 0,
            pos: None,
            total: 0,
        });
        window.focus(&handle, cx);
        cx.notify();
    }

    pub(crate) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.take().is_some() {
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    /// The field's text changed: recount when the query really differs (the
    /// field also notifies on caret moves, which must not jump the view).
    fn search_sync(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(search) = &mut self.search else {
            return;
        };
        if search.query == text {
            return;
        }
        search.query = text;
        self.search_update(cx);
    }

    /// The query changed: recount and jump to the newest match.
    fn search_update(&mut self, cx: &mut Context<Self>) {
        let Some(hits) = self.search_hits() else {
            return;
        };
        let search = self.search.as_mut().unwrap();
        search.total = hits.len();
        search.index = hits.len().saturating_sub(1);
        search.pos = hits.last().copied();
        self.search_reveal(cx);
    }

    pub(crate) fn search_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(hits) = self.search_hits() else {
            return;
        };
        let search = self.search.as_mut().unwrap();
        search.total = hits.len();
        if hits.is_empty() {
            search.pos = None;
            return cx.notify();
        }
        let n = hits.len();
        search.index = if forward {
            (search.index + 1) % n
        } else {
            (search.index + n - 1) % n
        };
        search.pos = Some(hits[search.index]);
        self.search_reveal(cx);
    }

    // ponytail: rescans the whole buffer per keystroke; the history is capped
    // at MAX_SCROLLBACK lines, so this stays cheap.
    fn search_hits(&self) -> Option<Vec<(usize, usize)>> {
        let search = self.search.as_ref()?;
        let term = &self.tabs.get(self.active)?.emu.term;
        Some(all_hits(term, &search.query))
    }

    /// Scrolls so the current match sits mid-screen.
    fn search_reveal(&mut self, cx: &mut Context<Self>) {
        let Some((line, _)) = self.search.as_ref().and_then(|s| s.pos) else {
            return cx.notify();
        };
        let tab = &mut self.tabs[self.active];
        let history = tab.emu.term.history_len();
        let top = line.saturating_sub(tab.emu.term.rows / 2).min(history);
        tab.scroll = history - top;
        tab.scroll_acc = 0.;
        self.selection = None;
        self.selecting = false;
        cx.notify();
    }

    /// Match highlights for the visible rows, same trick as the selection:
    /// rectangles above the text.
    pub(crate) fn search_overlays(&self, term: &Term, top: usize) -> Vec<Div> {
        let Some(search) = &self.search else {
            return Vec::new();
        };
        let needle = needle(&search.query);
        let mut rects = Vec::new();
        for row in 0..term.rows {
            for col in line_hits(term.line(top + row), &needle) {
                let current = search.pos == Some((top + row, col));
                rects.push(
                    div()
                        .absolute()
                        .left(self.cell_width * col as f32)
                        .top(self.line_height * row as f32)
                        .w(self.cell_width * needle.len() as f32)
                        .h(self.line_height)
                        .bg(color_of(theme::ACCENT).opacity(if current { 0.6 } else { 0.3 })),
                );
            }
        }
        rects
    }

    /// The bar, over the top right of the focused pane: field, count,
    /// previous / next / close.
    pub(crate) fn search_bar(&self, cx: &mut Context<Self>) -> Option<Div> {
        let search = self.search.as_ref()?;
        let status = if search.query.is_empty() {
            String::new()
        } else if search.total == 0 {
            "No results".to_string()
        } else {
            format!("{} of {}", search.index + 1, search.total)
        };
        let bar = div()
            .id("search-bar")
            .flex()
            .items_center()
            .gap_1()
            .h(px(30.))
            .pl(px(8.))
            .pr(px(4.))
            .rounded(theme::CONTROL_RADIUS)
            .bg(color_of(theme::CHROME))
            .border_1()
            .border_color(color_of(theme::BORDER))
            // Clicks on the bar belong to the bar, not to the pane below.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(icon("icons/magnifying-glass.svg", 12., theme::MUTED))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_sm()
                    .child(search.input.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .px_1()
                    .text_xs()
                    .text_color(color_of(theme::MUTED))
                    .child(status),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(1.))
                    .h(px(14.))
                    .bg(color_of(theme::BORDER)),
            )
            .child(bar_button(
                "search-prev",
                "icons/chevron-up.svg",
                cx,
                |v, _, cx| v.search_step(false, cx),
            ))
            .child(bar_button(
                "search-next",
                "icons/chevron-down.svg",
                cx,
                |v, _, cx| v.search_step(true, cx),
            ))
            .child(bar_button(
                "search-close",
                "icons/x-mark.svg",
                cx,
                |v, window, cx| v.close_search(window, cx),
            ));
        Some(
            div()
                .absolute()
                .top(px(8.))
                .left(px(8.))
                .right(px(16.))
                .flex()
                .justify_end()
                .child(div().flex_shrink(1.).w(px(360.)).min_w(px(0.)).child(bar)),
        )
    }
}

/// Icon button of the search bar. Its click is handled by `act` on the view.
fn bar_button(
    id: &'static str,
    path: &'static str,
    cx: &mut Context<TerminalView>,
    act: fn(&mut TerminalView, &mut Window, &mut Context<TerminalView>),
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .size(px(22.))
        .rounded(theme::CHIP_RADIUS)
        .cursor_pointer()
        .hover(|s| s.bg(color_of(theme::HOVER)))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| act(this, window, cx)))
        .child(icon(path, 12., theme::MUTED))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(text: &str) -> Vec<Cell> {
        text.chars()
            .map(|ch| Cell {
                ch,
                ..Cell::default()
            })
            .collect()
    }

    #[::core::prelude::v1::test]
    fn line_hits_is_case_insensitive_and_overlapping() {
        assert_eq!(line_hits(&cells("Foo foo"), &needle("FOO")), vec![0, 4]);
        assert_eq!(line_hits(&cells("aaa"), &needle("aa")), vec![0, 1]);
        assert!(line_hits(&cells("abc"), &needle("")).is_empty());
        assert!(line_hits(&cells("ab"), &needle("abc")).is_empty());
    }

    #[::core::prelude::v1::test]
    fn all_hits_spans_history_and_screen() {
        let mut emu = crate::terminal::Emulator::new(8, 2);
        emu.feed(b"one foo\r\ntwo\r\nfoo three");
        let term = &emu.term;
        let hits = all_hits(term, "foo");
        assert_eq!(hits.len(), 2);
        assert!(hits[0].0 < hits[1].0);
    }
}
