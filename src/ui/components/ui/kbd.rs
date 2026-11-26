//! Keyboard chord display, modelled on shadcn's `Kbd` / `KbdGroup`: one
//! small rounded key cap per key, joined by a muted `+`.

use crate::terminal::DEFAULT_FG;
use crate::ui::theme::{self, color_of};
use gpui::*;

/// One key cap.
pub(crate) fn kbd(label: &str) -> Div {
    div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .h(px(20.))
        .min_w(px(20.))
        .px(px(5.))
        .rounded(theme::CHIP_RADIUS)
        .border_1()
        .border_color(color_of(theme::BORDER))
        .bg(color_of(theme::ACTIVE))
        .text_xs()
        .text_color(color_of(DEFAULT_FG))
        .child(label.to_string())
}

/// A chord such as `Ctrl+Shift+T`: a cap per key with a `+` between them.
/// Text that is not a chord (e.g. "Press keys…") is shown as one cap.
pub(crate) fn kbd_chord(chord: &str) -> Div {
    let parts: Vec<&str> = chord.split('+').collect();
    let mut row = div().flex().items_center().gap_1();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            row = row.child(
                div()
                    .text_xs()
                    .text_color(color_of(theme::MUTED))
                    .child("+"),
            );
        }
        row = row.child(kbd(part));
    }
    row
}
