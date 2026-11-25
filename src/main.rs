#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use gpui::*;

mod app;
mod terminal;
mod ui;

use app::TerminalView;

pub(crate) const FONT: &str = if cfg!(windows) {
    "Consolas"
} else if cfg!(target_os = "macos") {
    "Menlo"
} else {
    "Noto Sans Mono"
};
pub(crate) const COLS: usize = 80;
pub(crate) const ROWS: usize = 24;
pub(crate) const WINDOW_SIZE: Size<Pixels> = size(px(696.), px(608.));
/// Window floor: below the size that shows 80×24 the terminal is clipped,
/// but the window stays resizable.
pub(crate) const MIN_WINDOW_SIZE: Size<Pixels> = size(px(320.), px(200.));

fn main() {
    app::startup::restrict_vulkan_drivers();
    app::startup::limit_allocator_arenas();
    gpui_platform::application()
        .with_assets(ui::assets::Assets)
        .run(|cx: &mut App| {
            ui::components::bind_text_input_keys(cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    WINDOW_SIZE,
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("ftty".into()),
                    appears_transparent: true,
                    traffic_light_position: if cfg!(target_os = "macos") {
                        Some(point(px(12.), px(8.)))
                    } else {
                        None
                    },
                }),
                window_decorations: Some(WindowDecorations::Client),
                window_background: WindowBackgroundAppearance::Transparent,
                window_min_size: Some(MIN_WINDOW_SIZE),
                ..Default::default()
            };
            cx.open_window(options, |window, cx| {
                cx.new(|cx| TerminalView::new(window, cx))
            })
            .unwrap();
            cx.activate(true);
        });
}
