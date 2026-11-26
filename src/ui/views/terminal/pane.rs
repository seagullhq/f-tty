//! Pane boxes of the active tab: each pane's text, the dimming of unfocused
//! panes and the corner tags of the focused one. Dividers live in `divider.rs`.

use super::text::line_from_cells;
use crate::app::TerminalView;
use crate::terminal::DEFAULT_BG;
use crate::ui::theme::{self, color_of};
use gpui::prelude::FluentBuilder;
use gpui::*;

/// One element per visible pane of the active tab, positioned in the tab's area.
pub(crate) fn pane_elements(
    view: &TerminalView,
    cx: &mut Context<TerminalView>,
) -> Vec<AnyElement> {
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
        let search = if focused { view.search_bar(cx) } else { None };
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
                .child(
                    div()
                        .absolute()
                        .left(theme::PANE_INSET)
                        .top(theme::PANE_INSET)
                        .right(theme::PANE_INSET)
                        .bottom(theme::PANE_INSET)
                        .children(lines)
                        .when(focused, |d| {
                            d.children(view.selection_overlays(term))
                                .children(view.search_overlays(term, top))
                        }),
                )
                .when(focused, |d| d.children(search).children(view.scrollbar()))
                // Unfocused panes fade, as in iTerm2, WezTerm, Ghostty and Warp.
                .when(split && !focused, |d| {
                    d.child(
                        div()
                            .absolute()
                            .size_full()
                            .bg(color_of(DEFAULT_BG).opacity(0.4)),
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

    children
}
