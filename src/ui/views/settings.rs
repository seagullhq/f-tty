//! Settings panel: font family, tab position and default shell.

use crate::app::{TabPosition, TerminalView};
use crate::terminal::DEFAULT_FG;
use crate::ui::theme::{self, color_of};
use gpui::prelude::FluentBuilder;
use gpui::*;

/// Right column of the settings form: bounded, so controls stay compact.
const CONTROL_WIDTH: Pixels = px(300.);

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
    /// Settings content: font family, tab position and default shell.
    pub(crate) fn settings(&self, cx: &mut Context<Self>) -> Div {
        let mut view = div()
            .flex_1()
            .w_full()
            .min_w(px(0.))
            .min_h(px(0.))
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

        view
    }
}
