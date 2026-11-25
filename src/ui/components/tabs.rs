//! Horizontal and vertical tab lists.

use crate::app::{TabPosition, TerminalView};
use crate::terminal::DEFAULT_FG;
use crate::ui::components::icon;
use crate::ui::theme::{self, color_of};
use gpui::prelude::FluentBuilder;
use gpui::*;

impl TerminalView {
    /// A horizontal tab (bar) or vertical tab (side panel).
    pub(crate) fn tab(&self, i: usize, vertical: bool, cx: &mut Context<Self>) -> impl IntoElement {
        // With Settings open no tab is active: the space belongs to settings.
        let active = !self.settings_open && i == self.active;
        let group = format!("tab-{i}");
        let tint = self.tabs[i].tint;
        let badge = div()
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .h(px(16.))
            .min_w(px(22.))
            .px(px(3.))
            .rounded(theme::CHIP_RADIUS)
            .bg(color_of(theme::BADGE_BG))
            .text_color(color_of(tint))
            .text_size(px(10.))
            .child(self.tabs[i].badge.clone())
            .when(self.tabs[i].attention, |b| {
                // Bell rang while this tab was hidden.
                b.child(
                    div()
                        .absolute()
                        .top(px(-2.))
                        .right(px(-2.))
                        .size(px(7.))
                        .rounded_full()
                        .bg(color_of(theme::ACCENT)),
                )
            });
        let compact = vertical && self.strip_width < theme::STRIP_COMPACT;

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
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_aux_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                if ev.is_middle_click() {
                    this.close_tab(i, window, cx);
                }
            }));

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
                .when(compact, |d| d.justify_center())
                .when(!compact, |d| d.pl(theme::TAB_PADDING))
                .min_w(px(0.))
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
                .gap_2()
                .child(badge)
                // The ellipsis needs a block-level text box; as a direct flex
                // child of the title the text would just be clipped.
                .when(!compact, |d| {
                    d.child(div().truncate().child(self.tabs[i].title.clone()))
                }),
        )
        .when(!compact, |t| t.child(close))
    }

    pub(crate) fn new_tab_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
    pub(crate) fn bottom_tabs(&self, cx: &mut Context<Self>) -> Div {
        let mut row = div().w_full().flex_none().flex().items_center().gap_1();

        for i in 0..self.tabs.len() {
            row = row.child(self.tab(i, false, cx));
        }

        row.child(self.new_tab_button(cx)).child(div().flex_1())
    }

    /// Vertical side tab list, browser style, as chips on a single surface.
    pub(crate) fn side_tabs(&self, cx: &mut Context<Self>) -> Div {
        let compact = self.strip_width < theme::STRIP_COMPACT;
        // Drag handle on the edge facing the terminal; it reaches into the gap.
        let handle = div()
            .id("strip-handle")
            .absolute()
            .top_0()
            .h_full()
            .w(px(8.))
            .cursor_col_resize()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    // Double click restores the default width.
                    if ev.click_count >= 2 {
                        this.strip_width = theme::STRIP_WIDTH;
                        this.strip_drag = None;
                        this.save_settings();
                        cx.notify();
                    } else {
                        this.strip_drag = Some((ev.position.x, this.strip_width));
                    }
                }),
            );
        let handle = if self.tabs_position == TabPosition::Left {
            handle.right(px(-4.))
        } else {
            handle.left(px(-4.))
        };
        let mut panel = div()
            .relative()
            .flex_none()
            .w(self.strip_width)
            .h_full()
            .flex()
            .flex_col()
            .gap_1()
            .child(handle)
            .child(
                div()
                    .h(theme::TAB_HEIGHT)
                    .w_full()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .when(compact, |d| d.justify_center())
                    .when(!compact, |d| d.pl(theme::TAB_PADDING).pr(px(2.)))
                    .text_xs()
                    .text_color(color_of(theme::MUTED))
                    .when(!compact, |d| d.child("Tabs"))
                    .child(self.new_tab_button(cx)),
            );

        for i in 0..self.tabs.len() {
            panel = panel.child(self.tab(i, true, cx));
        }

        panel
    }
}
