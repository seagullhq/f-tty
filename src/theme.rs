//! Design system tokens: flat, compact, rounded corners, single accent.
//!
//! Chrome is a single surface; the terminal is a rounded "card" a few pixels
//! away from the edges. No gradients, shadows or blur.

use gpui::{Pixels, px};

// Surfaces
pub const CHROME: u32 = 0x262626;
pub const HOVER: u32 = 0x303030;
pub const ACTIVE: u32 = 0x343434;
pub const BORDER: u32 = 0x3a3a3a;
pub const DANGER: u32 = 0xc42b1c;
pub const SELECTION: u32 = 0x264f78;

// Text
pub const MUTED: u32 = 0x9a9a9a;
pub const ACCENT: u32 = 0x4ec9b0;

// Compact measurements
pub const BAR_HEIGHT: Pixels = px(34.);
pub const BAR_PADDING: Pixels = px(6.);
pub const GAP: Pixels = px(4.);
pub const TAB_WIDTH: Pixels = px(132.);
pub const TAB_MIN_WIDTH: Pixels = px(80.);
pub const TAB_HEIGHT: Pixels = px(26.);
pub const TAB_PADDING: Pixels = px(8.);
pub const STRIP_WIDTH: Pixels = px(140.);
pub const CONTROL_SIZE: Pixels = px(24.);
pub const CONTROL_GAP: Pixels = px(2.);
pub const CONTENT_PADDING: Pixels = px(6.);

// Radii
pub const WINDOW_RADIUS: Pixels = px(12.);
pub const SURFACE_RADIUS: Pixels = px(8.);
pub const CONTROL_RADIUS: Pixels = px(6.);
pub const TAB_RADIUS: Pixels = px(6.);
pub const CHIP_RADIUS: Pixels = px(4.);
