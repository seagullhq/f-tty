//! Scrollback search: Ctrl+Shift+F opens a bar, Enter / Shift+Enter step
//! through matches, Esc closes it.

use crate::app::TerminalView;
use crate::terminal::{Cell, Term};
use crate::ui::theme::{self, color_of};
use gpui::*;

pub(crate) struct Search {
    pub(crate) query: String,
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
    pub(crate) fn open_search(&mut self, cx: &mut Context<Self>) {
        if self.search.is_none() && !self.settings_open {
            self.search = Some(Search {
                query: String::new(),
                index: 0,
                pos: None,
                total: 0,
            });
            cx.notify();
        }
    }

    pub(crate) fn close_search(&mut self, cx: &mut Context<Self>) {
        if self.search.take().is_some() {
            cx.notify();
        }
    }

    /// Handles a key while the bar is open; the shell never sees it.
    pub(crate) fn search_key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) {
        let Some(search) = &mut self.search else {
            return;
        };
        match ks.key.as_str() {
            "escape" => return self.close_search(cx),
            "enter" => return self.search_step(!ks.modifiers.shift, cx),
            "backspace" => {
                search.query.pop();
            }
            _ => match Self::typed_text(ks) {
                Some(text) if !ks.modifiers.control && !ks.modifiers.platform => {
                    search.query.push_str(&text)
                }
                _ => return,
            },
        }
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

    fn search_step(&mut self, forward: bool, cx: &mut Context<Self>) {
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

    /// The bar, pinned to the top right of the terminal card.
    pub(crate) fn search_bar(&self) -> Option<Div> {
        let search = self.search.as_ref()?;
        let status = if search.query.is_empty() {
            String::new()
        } else if search.total == 0 {
            "No results".to_string()
        } else {
            format!("{}/{}", search.index + 1, search.total)
        };
        Some(
            div()
                .absolute()
                .top(px(8.))
                .right(px(16.))
                .flex()
                .items_center()
                .gap_3()
                .h(px(30.))
                .px(px(10.))
                .rounded(theme::CONTROL_RADIUS)
                .bg(color_of(theme::ACTIVE))
                .border_1()
                .border_color(color_of(theme::BORDER))
                .text_xs()
                .child(
                    div()
                        .min_w(px(140.))
                        .text_color(color_of(crate::terminal::DEFAULT_FG))
                        .child(format!("{}▏", search.query)),
                )
                .child(div().text_color(color_of(theme::MUTED)).child(status)),
        )
    }
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
