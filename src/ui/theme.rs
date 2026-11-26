//! Design system tokens: flat, compact, rounded corners, single accent.
//!
//! Chrome is a single surface; the terminal is a rounded "card" a few pixels
//! away from the edges. No gradients, shadows or blur.

use gpui::{Hsla, Pixels, px, rgb};

/// Converts a packed 0xRRGGBB token into a GPUI color.
pub fn color_of(c: u32) -> Hsla {
    rgb(c).into()
}

// Surfaces
pub const CHROME: u32 = 0x222222;
pub const HOVER: u32 = 0x2c2c2c;
pub const ACTIVE: u32 = 0x303030;
pub const BORDER: u32 = 0x363636;
pub const DANGER: u32 = 0xc42b1c;
pub const SELECTION: u32 = 0x264f78;

// Text
/// Solid fill behind tab badges: dark enough for every tab color to read.
pub const BADGE_BG: u32 = 0x141414;
pub const MUTED: u32 = 0x8c8c8c;
pub const ACCENT: u32 = 0x4ec9b0;

// Compact measurements
pub const BAR_HEIGHT: Pixels = px(34.);
pub const BAR_PADDING: Pixels = px(6.);
pub const GAP: Pixels = px(4.);
pub const TAB_WIDTH: Pixels = px(132.);
pub const TAB_MIN_WIDTH: Pixels = px(80.);
pub const TAB_HEIGHT: Pixels = px(26.);
pub const TAB_PADDING: Pixels = px(8.);
/// Inset between a pane edge and its text, so text never touches a border.
pub const PANE_INSET: Pixels = px(8.);
/// Hairline between split panes at rest; panes touch the divider.
pub const SPLIT_GAP: Pixels = px(1.);
/// Width of the divider handle shown on hover and drag, centred on the hairline.
pub const SPLIT_BAR: Pixels = px(2.);
/// Grab area across a divider: wider than the gap so it is easy to hit.
pub const SPLIT_HIT: Pixels = px(10.);
pub const STRIP_WIDTH: Pixels = px(140.);
/// Side tab list limits: at the minimum only a letter per tab is shown.
pub const STRIP_MIN: Pixels = px(40.);
pub const STRIP_MAX: Pixels = px(320.);
/// Below this width the side list drops titles and close buttons.
pub const STRIP_COMPACT: Pixels = px(72.);
pub const CONTROL_SIZE: Pixels = px(24.);
pub const CONTROL_GAP: Pixels = px(2.);
pub const CONTENT_PADDING: Pixels = px(6.);
/// Top inset of the content card, above the terminal panes.
pub const CONTENT_TOP: Pixels = px(6.);

/// Accent per tab, picked by tab id so neighbours differ.
pub const TAB_COLORS: [u32; 8] = [
    0x4fc1ff, 0xb5cea8, 0xe6a37e, 0xd7a0d2, 0xdcdcaa, 0x4ec9b0, 0xff8f88, 0x9cdcfe,
];

// Radii
pub const WINDOW_RADIUS: Pixels = px(12.);
pub const SURFACE_RADIUS: Pixels = px(8.);
pub const CONTROL_RADIUS: Pixels = px(6.);
pub const TAB_RADIUS: Pixels = px(6.);
pub const CHIP_RADIUS: Pixels = px(4.);
