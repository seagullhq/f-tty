//! Top bar composition.

use crate::app::{TabPosition, TerminalView};
use crate::terminal::DEFAULT_FG;
use crate::ui::components::{icon, window_controls};
use crate::ui::theme::{self, color_of};
use gpui::*;

impl TerminalView {
    /// Top bar: brand, tabs (if at top), drag area, layout selector, window
    /// controls. Active tabs share the terminal background.
    pub(crate) fn chrome(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let mut bar = div()
            .h(theme::BAR_HEIGHT)
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .pl(theme::BAR_PADDING)
            .pr(if cfg!(windows) {
                px(0.)
            } else {
                theme::BAR_PADDING
            });

        // On Windows dragging goes through WM_NCHITTEST (WindowControlArea),
        // not through client mouse events.
        #[cfg(not(windows))]
        {
            bar = bar.on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move());
        }

        // macOS: space for the native traffic lights at the top left.
        if cfg!(target_os = "macos") {
            bar = bar.child(div().flex_none().w(px(66.)));
        }

        bar = bar.child(
            div()
                .flex_none()
                .w(px(2.))
                .h(px(12.))
                .rounded(px(1.))
                .bg(color_of(theme::ACCENT)),
        );

        if self.tabs_position == TabPosition::Top {
            for i in 0..self.tabs.len() {
                bar = bar.child(self.tab(i, false, cx));
            }
            bar = bar.child(self.new_tab_button(cx));
        } else {
            bar = bar.child(
                div()
                    .h_full()
                    .flex()
                    .items_center()
                    .text_xs()
                    .text_color(color_of(theme::MUTED))
                    .window_control_area(WindowControlArea::Drag)
                    .child("ftty"),
            );
        }

        let right = if cfg!(target_os = "macos") {
            // Window controls handled by the system (traffic lights).
            div().flex().items_center().child(self.settings_button(cx))
        } else {
            div()
                .flex()
                .items_center()
                .h_full()
                .gap(theme::CONTROL_GAP)
                .child(self.settings_button(cx))
                .child(window_controls(window))
        };

        bar.child(
            div()
                .flex_1()
                .h_full()
                .window_control_area(WindowControlArea::Drag),
        )
        .child(right)
    }

    /// Opens/closes the settings view.
    fn settings_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.settings_open;
        div()
            .id("settings")
            .flex()
            .items_center()
            .justify_center()
            .flex_none()
            .w(theme::CONTROL_SIZE)
            .h(theme::CONTROL_SIZE)
            .rounded(theme::CONTROL_RADIUS)
            .bg(color_of(if active { theme::ACTIVE } else { theme::CHROME }))
            .cursor_pointer()
            .hover(|s| s.bg(color_of(theme::HOVER)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| this.toggle_settings(cx)))
            .child(icon(
                "icons/cog-6-tooth.svg",
                14.,
                if active { DEFAULT_FG } else { theme::MUTED },
            ))
    }
}
