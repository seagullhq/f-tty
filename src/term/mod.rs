//! ANSI terminal emulator: color palette, screen grid and VT parser.

mod color;
mod grid;
mod parser;

#[cfg(test)]
mod tests;

pub use color::{Cell, Color, Style};
pub use grid::Term;
pub use parser::Emulator;

pub const DEFAULT_FG: u32 = 0xd4d4d4;
pub const DEFAULT_BG: u32 = 0x1e1e1e;
/// History lines per tab: enough to go back through logs without growing
/// RAM too much (~2 MB/tab at 80 columns).
pub const MAX_SCROLLBACK: usize = 2_000;
