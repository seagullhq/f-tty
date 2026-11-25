//! Primitive building blocks: icon, card, text input and combobox.

use crate::terminal::DEFAULT_BG;
use crate::ui::theme::{self, color_of};
use gpui::*;

mod combobox;
mod text_input;

pub(crate) use combobox::Combobox;
pub(crate) use text_input::{TextInput, bind_keys as bind_text_input_keys};

/// Monochrome Heroicons icon, tinted with the theme color.
pub(crate) fn icon(path: &'static str, size: f32, color: u32) -> Svg {
    svg()
        .path(path)
        .w(px(size))
        .h(px(size))
        .text_color(color_of(color))
}

/// Rounded surface a few pixels inside the window edge. The terminal grid and
/// the settings panel are both hosted in one.
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
        .child(content)
}
