//! Settings panel: font family, tab position, default shell and keyboard shortcuts.

use crate::app::scrollbar::SETTINGS_SCROLLBAR_WIDTH;
use crate::app::{TabPosition, TerminalView};
use crate::terminal::DEFAULT_FG;
use crate::ui::theme::{self, color_of};
use gpui::prelude::FluentBuilder;
use gpui::*;

/// Right column of the settings form: bounded, so controls stay compact.
const CONTROL_WIDTH: Pixels = px(300.);

/// Mouse actions stay fixed: they are not keyboard chords.
const MOUSE_BINDS: &[(&str, &str)] = &[
    ("Close tab", "Middle-click on tab"),
    ("Open link", "Ctrl+click"),
    ("Open / close settings", "Gear button"),
];

/// One shortcut row: same surface and radius as the other controls, with the
/// keys in a badge like the tab labels.
fn keybind_row(index: usize, label: &'static str, badge: impl IntoElement, capturing: bool) -> Div {
    div()
        .flex()
        .items_center()
        .gap_3()
        .min_h(px(28.))
        .px(px(10.))
        .bg(color_of(if capturing {
            theme::ACTIVE
        } else {
            theme::CHROME
        }))
        .when(index > 0 && !capturing, |d| {
            d.border_t_1().border_color(color_of(theme::BORDER))
        })
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .truncate()
                .text_color(color_of(theme::MUTED))
                .child(label),
        )
        .child(badge)
}

/// One row: label and hint on the left, control in a compact column.
/// Rows are separated by a hairline.
fn setting_row(label: &'static str, hint: &'static str, control: impl IntoElement) -> Div {
    div()
        .flex()
        .items_start()
        .gap(px(24.))
        .py(px(16.))
        .border_t_1()
        .border_color(color_of(theme::BORDER))
        .child(
            div()
                .flex_none()
                .w(px(180.))
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(color_of(DEFAULT_FG))
                        .child(label),
                )
                .child(div().text_color(color_of(theme::MUTED)).child(hint)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.))
                .max_w(CONTROL_WIDTH)
                .child(control),
        )
}

/// Selector icon: rectangle with the band on the tab list side.
fn layout_icon(position: TabPosition) -> Div {
    let icon = div()
        .w(px(12.))
        .h(px(10.))
        .border_1()
        .border_color(color_of(theme::MUTED));
    let band = || div().bg(color_of(theme::MUTED));

    match position {
        TabPosition::Top => icon.flex().flex_col().child(band().w_full().h(px(3.))),
        TabPosition::Bottom => icon
            .flex()
            .flex_col()
            .justify_end()
            .child(band().w_full().h(px(3.))),
        TabPosition::Left => icon.flex().child(band().w(px(3.)).h_full()),
        TabPosition::Right => icon.flex().justify_end().child(band().w(px(3.)).h_full()),
    }
}

impl TerminalView {
    /// Settings content: font family, tab position, default shell and shortcuts.
    pub(crate) fn settings(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        // Remember what this frame drew; the measuring canvas asks for another
        // frame if the real viewport differs, so the thumb appears on open.
        self.settings_metrics.set((
            self.settings_scroll.bounds().size.height,
            self.settings_scroll.max_offset().y,
        ));
        let mut view = div()
            .w_full()
            .px(px(32.))
            .py(px(24.))
            .flex()
            .flex_col()
            .text_xs()
            .child(
                div()
                    .pb(px(16.))
                    .text_lg()
                    .text_color(color_of(DEFAULT_FG))
                    .child("Settings"),
            );

        let mut font_control = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(self.font_picker.render(
                "font-picker",
                self.font.as_str(),
                &self.fonts,
                cx,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    this.toggle_font_picker(window, cx);
                }),
                cx.processor(|this, family: String, window, cx| {
                    this.select_font(family, window, cx);
                }),
            ));
        if self.font.as_str() != crate::FONT {
            font_control = font_control.child(
                div()
                    .id("font-reset")
                    .text_xs()
                    .text_color(color_of(theme::MUTED))
                    .cursor_pointer()
                    .hover(|s| s.text_color(color_of(DEFAULT_FG)))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.select_font(crate::FONT.to_string(), window, cx);
                    }))
                    .child(format!("Reset to default ({})", crate::FONT)),
            );
        }
        view = view.child(setting_row(
            "Font family",
            "Terminal and interface font.",
            font_control,
        ));

        let mut positions = div().flex().flex_wrap().gap_1();
        for (i, position) in TabPosition::ALL.into_iter().enumerate() {
            let current = position == self.tabs_position;
            positions = positions.child(
                div()
                    .id(("settings-layout", i))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(30.))
                    .px(px(10.))
                    .rounded(theme::CONTROL_RADIUS)
                    .bg(color_of(if current {
                        theme::ACTIVE
                    } else {
                        theme::CHROME
                    }))
                    .text_color(color_of(if current { DEFAULT_FG } else { theme::MUTED }))
                    .cursor_pointer()
                    .hover(|s| s.bg(color_of(theme::HOVER)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_tab_position(position, window, cx);
                    }))
                    .child(layout_icon(position))
                    .child(position.label()),
            );
        }
        view = view.child(setting_row(
            "Tab position",
            "Where the tab list sits.",
            positions,
        ));

        // Radio group: all shells visible, one selected, applied immediately.
        let mut shells = div()
            .flex()
            .flex_col()
            .rounded(theme::CONTROL_RADIUS)
            .border_1()
            .border_color(color_of(theme::BORDER))
            .overflow_hidden();
        for (i, shell) in self.shells.iter().enumerate() {
            let current = shell.program == self.shell.program;
            let name = shell.name.clone();
            let program = shell.program.clone();
            let shell = shell.clone();
            let radio = div()
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .size(px(14.))
                .rounded_full()
                .border_1()
                .border_color(color_of(if current { theme::ACCENT } else { theme::MUTED }))
                .when(current, |d| {
                    d.child(
                        div()
                            .size(px(8.))
                            .rounded_full()
                            .bg(color_of(theme::ACCENT)),
                    )
                });
            shells = shells.child(
                div()
                    .id(("settings-shell", i))
                    .flex()
                    .items_center()
                    .gap_3()
                    .h(px(34.))
                    .px(px(10.))
                    .when(i > 0, |d| {
                        d.border_t_1().border_color(color_of(theme::BORDER))
                    })
                    .bg(color_of(if current {
                        theme::ACTIVE
                    } else {
                        theme::CHROME
                    }))
                    .text_color(color_of(if current { DEFAULT_FG } else { theme::MUTED }))
                    .cursor_pointer()
                    .hover(|s| s.bg(color_of(theme::HOVER)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_shell(shell.clone(), cx);
                    }))
                    .child(radio)
                    .child(name)
                    .child(div().flex_1())
                    .child(
                        div()
                            .truncate()
                            .text_color(color_of(theme::MUTED))
                            .child(program),
                    ),
            );
        }
        view = view.child(setting_row(
            "Default shell",
            "New tabs use the selected shell.",
            shells,
        ));

        let mut shortcuts = div()
            .flex()
            .flex_col()
            .rounded(theme::CONTROL_RADIUS)
            .border_1()
            .border_color(color_of(theme::BORDER))
            .overflow_hidden();
        for (i, binding) in self.keybinds.entries().iter().enumerate() {
            let capturing = self.rebinding == Some(i);
            let keys = if capturing {
                "Press keys…".to_string()
            } else {
                binding.chord.to_string()
            };
            let badge = div()
                .id(("keybind", i))
                .flex_none()
                .rounded(theme::CHIP_RADIUS)
                .border_1()
                .border_color(color_of(if capturing {
                    theme::ACCENT
                } else {
                    theme::BORDER
                }))
                .bg(color_of(if capturing {
                    theme::ACCENT
                } else {
                    theme::BADGE_BG
                }))
                .px(px(6.))
                .py(px(2.))
                .max_w(px(260.))
                .truncate()
                .text_color(color_of(if capturing {
                    theme::BADGE_BG
                } else {
                    DEFAULT_FG
                }))
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.start_rebind(i, window, cx);
                }))
                .child(keys);
            shortcuts = shortcuts.child(keybind_row(i, binding.label, badge, capturing));
        }
        for (i, (label, keys)) in MOUSE_BINDS.iter().enumerate() {
            let index = self.keybinds.entries().len() + i;
            let badge = div()
                .flex_none()
                .rounded(theme::CHIP_RADIUS)
                .border_1()
                .border_color(color_of(theme::BORDER))
                .bg(color_of(theme::BADGE_BG))
                .px(px(6.))
                .py(px(2.))
                .text_color(color_of(theme::MUTED))
                .child(*keys);
            shortcuts = shortcuts.child(keybind_row(index, *label, badge, false));
        }
        view = view
            .child(
                div()
                    .pt(px(20.))
                    .pb(px(2.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(2.))
                            .h(px(12.))
                            .rounded(px(1.))
                            .bg(color_of(theme::ACCENT)),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(color_of(DEFAULT_FG))
                            .child("Keyboard shortcuts"),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("keybinds-reset")
                            .text_xs()
                            .text_color(color_of(theme::MUTED))
                            .cursor_pointer()
                            .hover(|s| s.text_color(color_of(DEFAULT_FG)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.reset_keybinds(cx);
                            }))
                            .child("Reset defaults"),
                    ),
            )
            .child(
                div()
                    .pb(px(8.))
                    .text_color(color_of(theme::MUTED))
                    .child("Click a key to rebind, Esc cancels. Mouse actions are fixed."),
            )
            .child(shortcuts);

        let mut root = div()
            .id("settings-root")
            .relative()
            .flex_1()
            .w_full()
            .min_w(px(0.))
            .min_h(px(0.))
            .on_mouse_move(cx.listener(TerminalView::on_settings_scrollbar_move))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(TerminalView::on_settings_scrollbar_up),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(TerminalView::on_settings_scrollbar_up),
            )
            .child(
                div()
                    .id("settings-scroll")
                    .absolute()
                    .top(px(0.))
                    .left(px(0.))
                    .size_full()
                    .overflow_y_scroll()
                    .scrollbar_width(SETTINGS_SCROLLBAR_WIDTH)
                    .track_scroll(&self.settings_scroll)
                    .child(view),
            );

        // The scroll handle only knows its bounds after prepaint. Compare them
        // with what we drew and redraw once when the viewport or content changes.
        let metrics = self.settings_metrics.clone();
        let scroll = self.settings_scroll.clone();
        root = root.child(
            canvas(
                move |bounds, window, _| {
                    let drawn = metrics.get();
                    if drawn.0 != bounds.size.height || drawn.1 != scroll.max_offset().y {
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top(px(0.))
            .left(px(0.))
            .size_full(),
        );

        if let Some(bar) = self.settings_scrollbar() {
            root = root.child(bar.on_mouse_down(
                MouseButton::Left,
                cx.listener(TerminalView::on_settings_scrollbar_down),
            ));
        }

        root
    }
}
