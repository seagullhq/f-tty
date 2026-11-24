//! Client-side decorations and composition of the main window.

mod chrome;
mod screen;
mod settings;
mod tabs;
mod window;

use crate::app::{TabPosition, TerminalView};
use crate::term::DEFAULT_BG;
use crate::theme::{self, color_of};
use gpui::*;

/// Invisible edge zone: captures the drag and delegates resizing to the compositor.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn resize_handle(zone: Div, edge: ResizeEdge) -> Div {
    zone.on_mouse_down(MouseButton::Left, move |_, window, cx| {
        cx.stop_propagation();
        window.start_window_resize(edge);
    })
}

/// Monochrome Heroicons icon, tinted with the theme color.
fn icon(path: &'static str, size: f32, color: u32) -> Svg {
    svg()
        .path(path)
        .w(px(size))
        .h(px(size))
        .text_color(color_of(color))
}

pub(crate) fn content_card(content: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .w_full()
        .min_w(px(0.))
        .min_h(px(0.))
        .px(theme::CONTENT_PADDING)
        .pt(px(2.))
        .pb(theme::CONTENT_PADDING)
        .bg(color_of(DEFAULT_BG))
        .border_1()
        .border_color(color_of(theme::BORDER))
        .rounded(theme::SURFACE_RADIUS)
        .overflow_hidden()
        .child(content)
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
impl TerminalView {
    /// Invisible edges to resize the window without system decorations (CSD):
    /// 6px on the sides, 10px on the corners.
    fn resize_zones(&self) -> Div {
        let edge = px(6.);
        let corner = px(10.);

        let mut zones = div().absolute().top(px(0.)).left(px(0.)).size_full();

        // On Windows caption buttons take the whole top border: no resize
        // zones there.
        if !cfg!(windows) {
            zones = zones
                .child(resize_handle(
                    div()
                        .absolute()
                        .top(px(0.))
                        .left(px(0.))
                        .w_full()
                        .h(edge)
                        .cursor_ns_resize(),
                    ResizeEdge::Top,
                ))
                .child(resize_handle(
                    div()
                        .absolute()
                        .top(px(0.))
                        .left(px(0.))
                        .size(corner)
                        .cursor_nwse_resize(),
                    ResizeEdge::TopLeft,
                ))
                .child(resize_handle(
                    div()
                        .absolute()
                        .top(px(0.))
                        .right(px(0.))
                        .size(corner)
                        .cursor_nesw_resize(),
                    ResizeEdge::TopRight,
                ));
        }

        zones
            .child(resize_handle(
                div()
                    .absolute()
                    .bottom(px(0.))
                    .left(px(0.))
                    .w_full()
                    .h(edge)
                    .cursor_ns_resize(),
                ResizeEdge::Bottom,
            ))
            .child(resize_handle(
                div()
                    .absolute()
                    .top(px(0.))
                    .bottom(px(0.))
                    .left(px(0.))
                    .w(edge)
                    .cursor_ew_resize(),
                ResizeEdge::Left,
            ))
            .child(resize_handle(
                div()
                    .absolute()
                    .top(if cfg!(windows) {
                        theme::BAR_HEIGHT
                    } else {
                        px(0.)
                    })
                    .bottom(px(0.))
                    .right(px(0.))
                    .w(edge)
                    .cursor_ew_resize(),
                ResizeEdge::Right,
            ))
            .child(resize_handle(
                div()
                    .absolute()
                    .bottom(px(0.))
                    .left(px(0.))
                    .size(corner)
                    .cursor_nesw_resize(),
                ResizeEdge::BottomLeft,
            ))
            .child(resize_handle(
                div()
                    .absolute()
                    .bottom(px(0.))
                    .right(px(0.))
                    .size(corner)
                    .cursor_nwse_resize(),
                ResizeEdge::BottomRight,
            ))
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let area = self.terminal_area(window);
        let cols = ((area.width / self.cell_width).floor() as usize).max(1);
        let rows = ((area.height / self.line_height).floor() as usize).max(1);
        self.resize_active_terminal(cols, rows, cx);

        let card = if self.settings_open {
            content_card(self.settings(cx))
        } else {
            content_card(screen::terminal_screen(self, cx))
        };

        let body = match self.tabs_position {
            TabPosition::Top => div()
                .flex_1()
                .w_full()
                .min_w(px(0.))
                .min_h(px(0.))
                .flex()
                .flex_col()
                .pl(theme::GAP)
                .pr(theme::GAP)
                .pb(theme::GAP)
                .child(card),
            TabPosition::Bottom => div()
                .flex_1()
                .w_full()
                .min_w(px(0.))
                .min_h(px(0.))
                .flex()
                .flex_col()
                .gap(theme::GAP)
                .pl(theme::GAP)
                .pr(theme::GAP)
                .pb(theme::GAP)
                .child(card)
                .child(self.bottom_tabs(cx)),
            TabPosition::Left => div()
                .flex_1()
                .w_full()
                .min_w(px(0.))
                .min_h(px(0.))
                .flex()
                .gap(theme::GAP)
                .pl(theme::GAP)
                .pr(theme::GAP)
                .pb(theme::GAP)
                .child(self.side_tabs(cx))
                .child(card),
            TabPosition::Right => div()
                .flex_1()
                .w_full()
                .min_w(px(0.))
                .min_h(px(0.))
                .flex()
                .gap(theme::GAP)
                .pl(theme::GAP)
                .pr(theme::GAP)
                .pb(theme::GAP)
                .child(card)
                .child(self.side_tabs(cx)),
        };

        // Il focus deve stare nel body, non nella root: su Windows un elemento
        // con track_focus chiama prevent_default() su ogni mouse down e
        // GPUI 0.2.2 scarta WM_NCLBUTTONDOWN (HTCAPTION), bloccando il drag
        // della titlebar client-side.
        let body = body.track_focus(&self.focus);

        let root = div()
            .on_key_down(cx.listener(Self::on_key))
            // The font picker stops mouse downs inside itself; anything else
            // collapses the dropdown like a normal combobox.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.font_picker_open {
                        this.font_picker_open = false;
                        cx.notify();
                    }
                }),
            )
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .rounded(theme::WINDOW_RADIUS)
            .bg(color_of(theme::CHROME))
            .whitespace_nowrap()
            .overflow_hidden()
            .child(self.chrome(window, cx))
            .child(body);

        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let root = root.child(self.resize_zones());

        root
    }
}
