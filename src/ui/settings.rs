//! Settings panel: tab position, default shell and terminal font.

use super::icon;
use crate::app::{TabPosition, TerminalView};
use crate::fonts;
use crate::term::DEFAULT_FG;
use crate::theme::{self, color_of};
use gpui::prelude::StatefulInteractiveElement;
use gpui::*;

fn section_label(text: &'static str) -> Div {
    div()
        .text_xs()
        .text_color(color_of(theme::MUTED))
        .child(text)
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
    /// Settings content: tab position, default shell and font family.
    pub(crate) fn settings(&self, cx: &mut Context<Self>) -> Div {
        let mut view = div()
            .flex_1()
            .w_full()
            .min_w(px(0.))
            .min_h(px(0.))
            .overflow_hidden()
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .text_xs();

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
        view = view.child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(section_label("Tab position"))
                .child(positions),
        );

        let mut shells = div().flex().flex_col().gap_1();
        for (i, shell) in self.shells.iter().enumerate() {
            let current = shell.program == self.shell.program;
            let name = shell.name.clone();
            let program = shell.program.clone();
            let shell = shell.clone();
            shells = shells.child(
                div()
                    .id(("settings-shell", i))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(30.))
                    .px(px(8.))
                    .rounded(theme::CONTROL_RADIUS)
                    .text_color(color_of(if current { DEFAULT_FG } else { theme::MUTED }))
                    .cursor_pointer()
                    .hover(|s| s.bg(color_of(theme::HOVER)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_shell(shell.clone(), cx);
                    }))
                    .child(
                        div()
                            .flex_none()
                            .w(px(2.))
                            .h(px(12.))
                            .bg(color_of(if current {
                                theme::ACCENT
                            } else {
                                theme::CHROME
                            })),
                    )
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
        view = view.child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(section_label("Default shell"))
                .child(shells)
                .child(
                    div()
                        .text_color(color_of(theme::MUTED))
                        .child("New tabs use the selected shell."),
                ),
        );

        view = view.child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(section_label("Font family"))
                .child(self.font_combobox(cx)),
        );

        view
    }

    /// Collapsed combobox: current family only; the search field and the
    /// result list appear together as a dropdown after clicking it.
    fn font_combobox(&self, cx: &mut Context<Self>) -> Div {
        let open = self.font_picker_open;
        let mut combo = div().relative().flex().flex_col().child(
            div()
                .id("font-picker-trigger")
                .flex()
                .items_center()
                .gap_2()
                .h(px(30.))
                .px(px(8.))
                .rounded(theme::CONTROL_RADIUS)
                .bg(color_of(theme::ACTIVE))
                .border_1()
                .border_color(color_of(if open { theme::ACCENT } else { theme::BORDER }))
                .cursor_pointer()
                .hover(|s| s.bg(color_of(theme::HOVER)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.font_picker_open = !this.font_picker_open;
                    cx.notify();
                }))
                .child(icon("icons/magnifying-glass.svg", 12., theme::MUTED))
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .text_color(color_of(theme::ACCENT))
                        .child(self.font.clone()),
                )
                .child(icon(
                    if open {
                        "icons/chevron-up.svg"
                    } else {
                        "icons/chevron-down.svg"
                    },
                    12.,
                    theme::MUTED,
                )),
        );

        if !open {
            return combo;
        }

        let font_matches = fonts::filter(&self.fonts, &self.font_query);
        combo = combo.child(
            div()
                .id("font-picker-popup")
                .absolute()
                .top(px(34.))
                .left(px(0.))
                .w_full()
                .flex()
                .flex_col()
                .gap_2()
                .p(px(8.))
                .rounded(theme::CONTROL_RADIUS)
                .bg(color_of(theme::CHROME))
                .border_1()
                .border_color(color_of(theme::BORDER))
                // Keep clicks inside the dropdown from reaching the root
                // handler that closes it.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(self.font_search_box())
                .child(
                    div()
                        .id("font-list")
                        .flex()
                        .flex_col()
                        .gap_1()
                        .max_h(px(160.))
                        .overflow_y_scroll()
                        .children(self.font_rows(&font_matches, cx)),
                )
                .child(div().text_color(color_of(theme::MUTED)).child(format!(
                    "{} matches · Enter applies the first match · Esc closes",
                    font_matches.len()
                ))),
        );

        combo
    }

    fn font_search_box(&self) -> Div {
        let query = &self.font_query;
        div()
            .flex()
            .items_center()
            .gap_2()
            .h(px(30.))
            .px(px(8.))
            .rounded(theme::CONTROL_RADIUS)
            .bg(color_of(theme::ACTIVE))
            .border_1()
            .border_color(color_of(theme::BORDER))
            .child(icon("icons/magnifying-glass.svg", 13., theme::MUTED))
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_color(color_of(if query.is_empty() {
                        theme::MUTED
                    } else {
                        DEFAULT_FG
                    }))
                    .child(if query.is_empty() {
                        "Search fonts…".to_string()
                    } else {
                        query.clone()
                    }),
            )
    }

    fn font_rows(&self, matches: &[&str], cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        if matches.is_empty() {
            return vec![
                div()
                    .id("font-list-empty")
                    .text_color(color_of(theme::MUTED))
                    .child("No fonts found"),
            ];
        }

        matches
            .iter()
            .copied()
            .take(120)
            .enumerate()
            .map(|(i, name)| {
                let current = name == self.font.as_str();
                let family = name.to_string();
                let selected = family.clone();
                div()
                    .id(("settings-font", i))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(26.))
                    .px(px(8.))
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
                        this.select_font(selected.clone(), window, cx);
                    }))
                    .child(
                        div()
                            .flex_none()
                            .w(px(2.))
                            .h(px(12.))
                            .bg(color_of(if current {
                                theme::ACCENT
                            } else {
                                theme::CHROME
                            })),
                    )
                    .child(div().truncate().font_family(family.clone()).child(family))
            })
            .collect()
    }

    /// Types into the font search field while the combobox is open.
    pub(crate) fn on_font_search_key(
        &mut self,
        ks: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let m = &ks.modifiers;
        if m.control || m.alt || m.platform || m.function {
            return;
        }
        match ks.key.as_str() {
            "backspace" => {
                self.font_query.pop();
            }
            "escape" => {
                if self.font_query.is_empty() {
                    self.font_picker_open = false;
                } else {
                    self.font_query.clear();
                }
            }
            "enter" => {
                if let Some(name) = fonts::filter(&self.fonts, &self.font_query)
                    .first()
                    .map(|name| (*name).to_string())
                {
                    self.select_font(name, window, cx);
                }
                return;
            }
            "tab" | "up" | "down" | "left" | "right" | "home" | "end" | "insert" | "delete"
            | "pageup" | "pagedown" => return,
            _ => match TerminalView::typed_text(ks) {
                Some(ch) => self.font_query.push_str(&ch),
                None => return,
            },
        }
        cx.notify();
    }
}
