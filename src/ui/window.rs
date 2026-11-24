//! Caption buttons and maximize/restore behaviour.

use super::icon;
use crate::app::TerminalView;
use crate::term::DEFAULT_FG;
use crate::theme::{self, color_of};
use gpui::*;

/// On Windows `zoom_window()` only maximizes: the "restore" button needs
/// SW_RESTORE, which GPUI does not expose.
#[cfg(windows)]
fn restore_maximized(window: &Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    #[link(name = "user32")]
    unsafe extern "system" {
        fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    }

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        const SW_RESTORE: i32 = 9;
        unsafe {
            ShowWindow(handle.hwnd.get(), SW_RESTORE);
        }
    }
}

fn maximize_or_restore(window: &Window) {
    #[cfg(windows)]
    {
        if window.is_maximized() {
            restore_maximized(window);
        } else {
            window.zoom_window();
        }
    }
    #[cfg(not(windows))]
    window.zoom_window();
}

/// Zed-style window control: clickable area with hover, no system chrome.
fn window_control(
    id: &'static str,
    icon: impl IntoElement,
    hover_bg: u32,
    action: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let button = div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .cursor_pointer()
        .hover(move |s| s.bg(color_of(hover_bg)))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            action(window, cx);
        })
        .child(icon);

    // Linux: round buttons. Windows: full-height caption without gaps.
    #[cfg(windows)]
    {
        button.w(px(40.)).h_full()
    }
    #[cfg(not(windows))]
    {
        button
            .w(theme::CONTROL_SIZE)
            .h(theme::CONTROL_SIZE)
            .rounded_full()
    }
}

impl TerminalView {
    /// Caption buttons: round on Linux, full height on Windows.
    pub(super) fn window_controls(&self, window: &Window) -> impl IntoElement {
        let icon_size = if cfg!(windows) { 12. } else { 11. };
        let (maximize_id, maximize_icon) = if window.is_maximized() {
            ("restore", "icons/square-2-stack.svg")
        } else {
            ("maximize", "icons/square.svg")
        };
        div()
            .flex()
            .items_center()
            .h_full()
            .gap(if cfg!(windows) {
                px(0.)
            } else {
                theme::CONTROL_GAP
            })
            .child(window_control(
                "minimize",
                icon("icons/minus.svg", icon_size, theme::MUTED),
                theme::HOVER,
                |window, _| window.minimize_window(),
            ))
            .child(window_control(
                maximize_id,
                icon(maximize_icon, icon_size, theme::MUTED),
                theme::HOVER,
                |window, _| maximize_or_restore(window),
            ))
            .child(window_control(
                "close",
                icon("icons/x-mark.svg", icon_size, DEFAULT_FG),
                theme::DANGER,
                |window, _| window.remove_window(),
            ))
    }
}
