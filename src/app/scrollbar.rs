//! Vertical scrollbar over the terminal: shows where the view sits in the
//! history, click or drag to jump.

use crate::app::TerminalView;
use crate::ui::theme::{self, color_of};
use gpui::*;

const WIDTH: Pixels = px(10.);
const MIN_THUMB: Pixels = px(24.);

/// Space reserved by the settings scroll container for the overlay thumb.
pub(crate) const SETTINGS_SCROLLBAR_WIDTH: Pixels = WIDTH;

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

/// Settings scrollbar: the viewport is the track and `max` is the positive
/// distance between viewport and content. `offset` is negative as the view
/// moves down, matching `ScrollHandle`.
fn settings_thumb(track: f32, max: f32, offset: f32, min: f32) -> (f32, f32) {
    let height = (track * track / (track + max)).max(min).min(track);
    let progress = if max > 0. {
        (-offset / max).clamp(0., 1.)
    } else {
        0.
    };
    ((track - height) * progress, height)
}

/// Offset that puts the thumb's center under `y` on the settings track.
fn settings_scroll_for(y: f32, track: f32, max: f32, thumb_height: f32) -> Pixels {
    let usable = (track - thumb_height).max(1.);
    let progress = ((y - thumb_height / 2.) / usable).clamp(0., 1.);
    px(-max * progress)
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
        let y = f32::from(position.y - self.pane_origin().y).clamp(0., track);
        let scroll = scroll_for(y, track, term.rows, term.history_len());
        let tab = &mut self.tabs[self.active];
        if tab.scroll != scroll {
            tab.scroll = scroll;
            tab.scroll_acc = 0.;
            self.selection = None;
            cx.notify();
        }
    }

    /// (track height, max offset, thumb top, thumb height) for settings, when
    /// the content actually overflows.
    fn settings_geometry(&self) -> Option<(Pixels, Pixels, Pixels, Pixels)> {
        let track = self.settings_scroll.bounds().size.height;
        let max = self.settings_scroll.max_offset().y;
        if track <= px(0.) || max <= px(0.) {
            return None;
        }
        let (top, height) = settings_thumb(
            f32::from(track),
            f32::from(max),
            f32::from(self.settings_scroll.offset().y),
            f32::from(MIN_THUMB),
        );
        Some((track, max, px(top), px(height)))
    }

    /// Visible settings scrollbar, overlaid on the right edge of the panel.
    pub(crate) fn settings_scrollbar(&self) -> Option<Stateful<Div>> {
        let (track, _, top, height) = self.settings_geometry()?;
        Some(
            div()
                .id("settings-scrollbar")
                .absolute()
                .top(px(0.))
                .right(px(0.))
                .w(WIDTH)
                .h(track)
                .bg(color_of(theme::CHROME))
                .border_l_1()
                .border_color(color_of(theme::BORDER))
                .cursor_pointer()
                .child(
                    div()
                        .absolute()
                        .top(top)
                        .left(px(2.))
                        .w(px(6.))
                        .h(height)
                        .rounded_full()
                        .bg(color_of(if self.settings_scroll_drag {
                            theme::MUTED
                        } else {
                            theme::BORDER
                        })),
                ),
        )
    }

    /// Starts a scrollbar drag; clicking the track jumps to that position too.
    pub(crate) fn on_settings_scrollbar_down(
        &mut self,
        ev: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if ev.button != MouseButton::Left {
            return;
        }
        self.settings_scroll_drag = true;
        self.scroll_settings_to(ev.position.y, cx);
    }

    /// Keeps the thumb under the pointer while the left button is held.
    pub(crate) fn on_settings_scrollbar_move(
        &mut self,
        ev: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.settings_scroll_drag {
            return;
        }
        if ev.pressed_button != Some(MouseButton::Left) {
            self.settings_scroll_drag = false;
            cx.notify();
            return;
        }
        self.scroll_settings_to(ev.position.y, cx);
    }

    pub(crate) fn on_settings_scrollbar_up(
        &mut self,
        _ev: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if std::mem::take(&mut self.settings_scroll_drag) {
            cx.notify();
        }
    }

    fn scroll_settings_to(&mut self, pointer_y: Pixels, cx: &mut Context<Self>) {
        let Some((track, max, _, thumb_height)) = self.settings_geometry() else {
            return;
        };
        let y =
            f32::from(pointer_y - self.settings_scroll.bounds().top()).clamp(0., f32::from(track));
        let offset =
            settings_scroll_for(y, f32::from(track), f32::from(max), f32::from(thumb_height));
        if self.settings_scroll.offset().y != offset {
            self.settings_scroll.set_offset(point(px(0.), offset));
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

    #[::core::prelude::v1::test]
    fn settings_thumb_tracks_the_scroll_offset() {
        // 100px viewport, 300px of overflow: thumb is 25px and spans the track.
        assert_eq!(settings_thumb(100., 300., 0., 10.), (0., 25.));
        assert_eq!(
            settings_thumb(100., 300., f32::from(px(-300.)), 10.),
            (75., 25.)
        );
        // A tiny viewport still gets the minimum thumb height.
        assert_eq!(settings_thumb(100., 10_000., 0., 24.).1, 24.);
    }

    #[::core::prelude::v1::test]
    fn settings_scroll_for_maps_track_ends_to_offset_ends() {
        assert_eq!(settings_scroll_for(0., 100., 300., 25.), px(0.));
        assert_eq!(settings_scroll_for(50., 100., 300., 25.), px(-150.));
        assert_eq!(settings_scroll_for(100., 100., 300., 25.), px(-300.));
    }
}
