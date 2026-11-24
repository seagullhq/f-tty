//! Window and terminal geometry for the four tab positions.

use crate::MIN_WINDOW_SIZE;
use crate::app::{TabPosition, TerminalView};
use crate::theme;
use gpui::*;

impl TerminalView {
    pub(crate) fn text_origin(&self) -> Point<Pixels> {
        let mut x = theme::GAP + px(1.) + theme::CONTENT_PADDING;
        if self.tabs_position == TabPosition::Left {
            x += theme::STRIP_WIDTH + theme::GAP;
        }
        point(x, theme::BAR_HEIGHT + px(1.) + px(2.))
    }

    pub(crate) fn terminal_area(&self, window: &Window) -> Size<Pixels> {
        let bounds = window.bounds().size;
        let (strip_w, strip_h) = match self.tabs_position {
            TabPosition::Top => (px(0.), px(0.)),
            TabPosition::Bottom => (px(0.), theme::TAB_HEIGHT + theme::GAP),
            TabPosition::Left | TabPosition::Right => (theme::STRIP_WIDTH + theme::GAP, px(0.)),
        };
        let width = bounds.width - theme::GAP * 2. - strip_w - px(2.) - theme::CONTENT_PADDING * 2.;
        let height = bounds.height
            - theme::BAR_HEIGHT
            - theme::GAP
            - strip_h
            - px(2.)
            - px(2.)
            - theme::CONTENT_PADDING;
        size(
            if width > px(1.) { width } else { px(1.) },
            if height > px(1.) { height } else { px(1.) },
        )
    }
}

/// Active index after closing `removed`, with at least one tab left.
pub(crate) fn active_after_close(active: usize, removed: usize, remaining: usize) -> usize {
    let active = if removed < active { active - 1 } else { active };
    active.min(remaining - 1)
}

/// Minimum window size for the current layout: side tabs need more width,
/// bottom tabs need more height.
pub(crate) fn min_size(position: TabPosition) -> Size<Pixels> {
    match position {
        TabPosition::Top => MIN_WINDOW_SIZE,
        TabPosition::Bottom => size(
            MIN_WINDOW_SIZE.width,
            MIN_WINDOW_SIZE.height + theme::TAB_HEIGHT + theme::GAP,
        ),
        TabPosition::Left | TabPosition::Right => size(
            MIN_WINDOW_SIZE.width + theme::STRIP_WIDTH + theme::GAP,
            MIN_WINDOW_SIZE.height,
        ),
    }
}

/// Window size delta when the tab list changes side, so the terminal keeps
/// the same columns/rows.
pub(crate) fn size_delta(old: TabPosition, new: TabPosition) -> (Pixels, Pixels) {
    let width = match (old.vertical(), new.vertical()) {
        (true, false) => -(theme::STRIP_WIDTH + theme::GAP),
        (false, true) => theme::STRIP_WIDTH + theme::GAP,
        _ => px(0.),
    };
    let height = match (old == TabPosition::Bottom, new == TabPosition::Bottom) {
        (true, false) => -(theme::TAB_HEIGHT + theme::GAP),
        (false, true) => theme::TAB_HEIGHT + theme::GAP,
        _ => px(0.),
    };
    (width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn minimum_size_depends_on_tab_position() {
        use TabPosition::*;
        assert_eq!(min_size(Top), MIN_WINDOW_SIZE);
        assert_eq!(min_size(Bottom).width, MIN_WINDOW_SIZE.width);
        assert_eq!(
            min_size(Bottom).height,
            MIN_WINDOW_SIZE.height + theme::TAB_HEIGHT + theme::GAP
        );
        assert_eq!(
            min_size(Left).width,
            MIN_WINDOW_SIZE.width + theme::STRIP_WIDTH + theme::GAP
        );
        assert_eq!(min_size(Left).height, MIN_WINDOW_SIZE.height);
    }

    #[::core::prelude::v1::test]
    fn layout_change_keeps_terminal_size() {
        use TabPosition::*;
        assert_eq!(
            size_delta(Top, Bottom),
            (px(0.), theme::TAB_HEIGHT + theme::GAP)
        );
        assert_eq!(
            size_delta(Bottom, Top),
            (px(0.), -(theme::TAB_HEIGHT + theme::GAP))
        );
        assert_eq!(
            size_delta(Top, Left),
            (theme::STRIP_WIDTH + theme::GAP, px(0.))
        );
        assert_eq!(size_delta(Left, Right), (px(0.), px(0.)));
        assert_eq!(
            size_delta(Right, Bottom),
            (
                -(theme::STRIP_WIDTH + theme::GAP),
                theme::TAB_HEIGHT + theme::GAP
            )
        );
    }

    #[::core::prelude::v1::test]
    fn active_index_survives_tab_close() {
        // Close a tab on the left: the active index shifts by one.
        assert_eq!(active_after_close(2, 0, 3), 1);
        // Close the active tab in the middle: it stays at the same position (next one).
        assert_eq!(active_after_close(1, 1, 2), 1);
        // Close the last active tab: switch to the previous one.
        assert_eq!(active_after_close(2, 2, 2), 1);
        assert_eq!(active_after_close(0, 0, 1), 0);
    }
}
