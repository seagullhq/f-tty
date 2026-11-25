//! Vertical scrollbar over the terminal: shows where the view sits in the
//! history, click or drag to jump.

use crate::app::TerminalView;
use crate::ui::theme::{self, color_of};
use gpui::*;

const WIDTH: Pixels = px(10.);
const MIN_THUMB: Pixels = px(24.);

/// Thumb (top, height) in a track of `track` px for a view of `rows` lines
/// starting at buffer line `top`, out of `history + rows` lines.
fn thumb(track: f32, rows: usize, history: usize, top: usize, min: f32) -> (f32, f32) {
    let total = (history + rows) as f32;
    let height = (track * rows as f32 / total).max(min).min(track);
    let y = (track - height) * top as f32 / history.max(1) as f32;
    (y, height)
}

/// Scroll offset (lines back from the present) that puts the view's
/// center at `y` px along the track.
fn scroll_for(y: f32, track: f32, rows: usize, history: usize) -> usize {
    let total = (history + rows) as f32;
    let top = (y / track * total - rows as f32 / 2.).clamp(0., history as f32);
    history - top.round() as usize
}

impl TerminalView {
    /// Track and thumb, only when there is history to scroll through.
    pub(crate) fn scrollbar(&self) -> Option<Div> {
        let term = &self.tabs.get(self.active)?.emu.term;
        let history = term.history_len();
        if history == 0 || term.in_alt_screen() {
            return None;
        }
        let track = self.line_height * term.rows as f32;
        let top = term.visible_top(self.tabs[self.active].scroll);
        let (y, height) = thumb(
            f32::from(track),
            term.rows,
            history,
            top,
            f32::from(MIN_THUMB),
        );
        let active = self.scrollbar_drag;
        Some(
            div()
                .absolute()
                .top(px(0.))
                .right(px(0.))
                .w(WIDTH)
                .h(track)
                .child(
                    div()
                        .absolute()
                        .top(px(y))
                        .left(px(2.))
                        .w(px(6.))
                        .h(px(height))
                        .rounded_full()
                        .bg(color_of(if active { theme::MUTED } else { theme::BORDER })),
                ),
        )
    }

    /// True when the position (window coordinates) is over the focused pane's
    /// scrollbar column.
    pub(crate) fn on_scrollbar(&self, position: Point<Pixels>) -> bool {
        let pane = &self.tabs[self.active];
        let right = pane.origin.x + pane.area.width;
        let term = &pane.emu.term;
        term.history_len() > 0 && !term.in_alt_screen() && position.x >= right - WIDTH
    }

    /// Moves the view to where the pointer is along the track.
    pub(crate) fn scrollbar_to(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let term = &self.tabs[self.active].emu.term;
        let track = f32::from(self.line_height * term.rows as f32);
        let y = f32::from(position.y - self.text_origin().y).clamp(0., track);
        let scroll = scroll_for(y, track, term.rows, term.history_len());
        let tab = &mut self.tabs[self.active];
        if tab.scroll != scroll {
            tab.scroll = scroll;
            tab.scroll_acc = 0.;
            self.selection = None;
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn thumb_spans_track_at_ends() {
        // 10 rows, 90 history: thumb is 10% of the track, min 5.
        assert_eq!(thumb(100., 10, 90, 0, 5.), (0., 10.));
        assert_eq!(thumb(100., 10, 90, 90, 5.), (90., 10.));
        // Tiny view still gets the minimum.
        assert_eq!(thumb(100., 1, 999, 0, 5.).1, 5.);
    }

    #[::core::prelude::v1::test]
    fn scroll_for_maps_track_ends_to_history_ends() {
        assert_eq!(scroll_for(0., 100., 10, 90), 90); // top of track = oldest
        assert_eq!(scroll_for(100., 100., 10, 90), 0); // bottom = present
    }
}
