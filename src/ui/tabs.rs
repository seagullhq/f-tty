//! Horizontal and vertical tab lists.

use super::icon;
use crate::app::TerminalView;
use crate::term::DEFAULT_FG;
use crate::theme::{self, color_of};
use gpui::*;

impl TerminalView {
    /// A horizontal tab (bar) or vertical tab (side panel).
    pub(super) fn tab(&self, i: usize, vertical: bool, cx: &mut Context<Self>) -> impl IntoElement {
        // With Settings open no tab is active: the space belongs to settings.
        let active = !self.settings_open && i == self.active;
        let group = format!("tab-{i}");

        let close = div()
            .id(("tab-close", i))
            .flex()
            .items_center()
            .justify_center()
            .w(px(16.))
            .h(px(16.))
            .mr(px(4.))
            .rounded(theme::CHIP_RADIUS)
            .cursor_pointer()
            .text_color(color_of(theme::MUTED))
            .hover(|s| {
                s.bg(color_of(theme::HOVER))
                    .text_color(color_of(DEFAULT_FG))
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, window, cx| {
                this.close_tab(i, window, cx);
            }))
            .child(icon("icons/x-mark.svg", 12., theme::MUTED));

        let close = if active {
            close
        } else {
            // On inactive tabs the × only appears on hover.
            close
                .group_hover(group.clone(), |s| s.opacity(1.))
                .opacity(0.)
        };

        let tab = div()
            .id(("tab", i))
            .group(group)
            .flex()
            .items_center()
            .h(theme::TAB_HEIGHT)
            .rounded(theme::TAB_RADIUS)
            .overflow_hidden()
            .bg(if active {
                color_of(theme::ACTIVE)
            } else {
                color_of(theme::CHROME)
            })
            .hover(move |s| {
                if active {
                    s
                } else {
                    s.bg(color_of(theme::HOVER))
                }
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());

        let tab = if vertical {
            tab.w_full()
        } else {
            tab.w(theme::TAB_WIDTH).min_w(theme::TAB_MIN_WIDTH)
        };

        tab.child(
            div()
                .id(("tab-title", i))
                .flex()
                .flex_1()
                .items_center()
                .h_full()
                .pl(theme::TAB_PADDING)
                .truncate()
                .cursor_pointer()
                .text_xs()
                .text_color(color_of(if active { DEFAULT_FG } else { theme::MUTED }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.active = i;
                    this.settings_open = false;
                    this.selection = None;
                    this.selecting = false;
                    cx.notify();
                }))
                .child(self.tabs[i].title.clone()),
        )
        .child(close)
    }

    pub(super) fn new_tab_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("new-tab")
            .flex()
            .items_center()
            .justify_center()
            .flex_none()
            .w(px(26.))
            .h(px(26.))
            .rounded(theme::TAB_RADIUS)
            .cursor_pointer()
            .text_size(px(16.))
            .text_color(color_of(theme::MUTED))
            .hover(|s| {
                s.bg(color_of(theme::HOVER))
                    .text_color(color_of(DEFAULT_FG))
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| this.open_tab(cx)))
            .child(icon("icons/plus.svg", 12., theme::MUTED))
    }

    /// Bottom tab list: rounded chips above the window edge.
    pub(super) fn bottom_tabs(&self, cx: &mut Context<Self>) -> Div {
        let mut row = div().w_full().flex_none().flex().items_center().gap_1();

        for i in 0..self.tabs.len() {
            row = row.child(self.tab(i, false, cx));
        }

        row.child(self.new_tab_button(cx)).child(div().flex_1())
    }

    /// Vertical side tab list, browser style, as chips on a single surface.
    pub(super) fn side_tabs(&self, cx: &mut Context<Self>) -> Div {
        let mut panel = div()
            .flex_none()
            .w(theme::STRIP_WIDTH)
            .h_full()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .h(theme::TAB_HEIGHT)
                    .w_full()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pl(theme::TAB_PADDING)
                    .pr(px(2.))
                    .text_xs()
                    .text_color(color_of(theme::MUTED))
                    .child("Tabs")
                    .child(self.new_tab_button(cx)),
            );

        for i in 0..self.tabs.len() {
            panel = panel.child(self.tab(i, true, cx));
        }

        panel
    }
}
