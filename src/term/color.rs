//! Terminal colors and character cell styling.

const ANSI16: [u32; 16] = [
    0x000000, 0xcd0000, 0x00cd00, 0xcdcd00, 0x0000ee, 0xcd00cd, 0x00cdcd, 0xe5e5e5, 0x7f7f7f,
    0xff0000, 0x00ff00, 0xffff00, 0x5c5cff, 0xff00ff, 0x00ffff, 0xffffff,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    pub fn resolve(self, default: u32) -> u32 {
        match self {
            Color::Default => default,
            Color::Rgb(r, g, b) => ((r as u32) << 16) | ((g as u32) << 8) | (b as u32),
            Color::Indexed(n) if n < 16 => ANSI16[n as usize],
            Color::Indexed(n) if n < 232 => {
                let n = n as u32 - 16;
                let c = |v: u32| if v == 0 { 0 } else { 55 + v * 40 };
                (c(n / 36) << 16) | (c((n / 6) % 6) << 8) | c(n % 6)
            }
            Color::Indexed(n) => {
                let g = 8 + (n as u32 - 232) * 10;
                (g << 16) | (g << 8) | g
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub underline: bool,
    pub inverse: bool,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            underline: false,
            inverse: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Cell {
    pub ch: char,
    pub style: Style,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            style: Style::default(),
        }
    }
}
