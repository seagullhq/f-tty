//! Window and terminal geometry for the four tab positions.

use crate::MIN_WINDOW_SIZE;
use crate::app::{TabPosition, TerminalView};
use crate::ui::theme;
use gpui::*;

impl TerminalView {
    /// Where the focused pane's first cell is (window coordinates).
    pub(crate) fn text_origin(&self) -> Point<Pixels> {
        self.tabs
            .get(self.active)
            .map_or(self.content_origin(), |t| t.origin)
    }

    /// Top-left of the terminal area, before panes are placed in it.
    pub(crate) fn content_origin(&self) -> Point<Pixels> {
        let mut x = theme::GAP + px(1.) + theme::CONTENT_PADDING;
        if self.tabs_position == TabPosition::Left {
            x += self.strip_width + theme::GAP;
        }
        point(x, theme::BAR_HEIGHT + px(1.) + px(2.))
    }

    pub(crate) fn terminal_area(&self, window: &Window) -> Size<Pixels> {
        let bounds = window.bounds().size;
        let (strip_w, strip_h) = match self.tabs_position {
            TabPosition::Top => (px(0.), px(0.)),
            TabPosition::Bottom => (px(0.), theme::TAB_HEIGHT + theme::GAP),
            TabPosition::Left | TabPosition::Right => (self.strip_width + theme::GAP, px(0.)),
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

/// Palette color used by the fewest open tabs (first one on ties), so
/// neighbours stay distinguishable until the palette runs out.
pub(crate) fn pick_tint(used: &[u32]) -> u32 {
    *theme::TAB_COLORS
        .iter()
        .min_by_key(|c| used.iter().filter(|u| u == c).count())
        .unwrap()
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
            MIN_WINDOW_SIZE.width + theme::STRIP_MIN + theme::GAP,
            MIN_WINDOW_SIZE.height,
        ),
    }
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
            MIN_WINDOW_SIZE.width + theme::STRIP_MIN + theme::GAP
        );
        assert_eq!(min_size(Left).height, MIN_WINDOW_SIZE.height);
    }

    fn luminance(c: u32) -> f32 {
        let lin = |v: u32| {
            let v = v as f32 / 255.;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c >> 16 & 0xff) + 0.7152 * lin(c >> 8 & 0xff) + 0.0722 * lin(c & 0xff)
    }

    fn contrast(a: u32, b: u32) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[::core::prelude::v1::test]
    fn tab_colors_are_readable_and_distinct() {
        for &tint in &theme::TAB_COLORS {
            assert!(contrast(tint, theme::BADGE_BG) >= 4.5, "{tint:06x} text");
            // The badge itself must stand out from the tab (non-text 3:1).
            assert!(contrast(tint, theme::CHROME) >= 3., "{tint:06x} shape");
        }
        let mut seen = std::collections::HashSet::new();
        assert!(theme::TAB_COLORS.iter().all(|c| seen.insert(c)));
    }

    #[::core::prelude::v1::test]
    fn pick_tint_spreads_then_reuses() {
        let first = pick_tint(&[]);
        let second = pick_tint(&[first]);
        assert_ne!(first, second);
        let all = theme::TAB_COLORS;
        assert_eq!(pick_tint(&all), all[0]);
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
