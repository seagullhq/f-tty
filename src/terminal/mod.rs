//! Terminal session: ANSI emulator, PTY and shell detection.

mod color;
mod grid;
mod integration;
mod parser;

pub(crate) mod pty;
pub(crate) mod shells;

#[cfg(test)]
mod tests;

pub use color::{Cell, Color, Style};
pub use grid::Term;
pub use parser::Emulator;

pub const DEFAULT_FG: u32 = 0xd8d8d8;
pub const DEFAULT_BG: u32 = 0x1a1a1a;
/// History lines per tab: enough to go back through logs without growing
/// RAM too much (~2 MB/tab at 80 columns).
pub const MAX_SCROLLBACK: usize = 2_000;
