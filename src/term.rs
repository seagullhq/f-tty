use std::collections::VecDeque;
use vte::{Params, Parser, Perform};

pub const DEFAULT_FG: u32 = 0xd4d4d4;
pub const DEFAULT_BG: u32 = 0x1e1e1e;
/// Righe di storico per tab: abbastanza per risalire nei log senza far
/// crescere troppo la RAM (~2 MB/tab a 80 colonne).
pub const MAX_SCROLLBACK: usize = 2_000;

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

pub struct Term {
    pub cols: usize,
    pub rows: usize,
    /// Schermo come buffer piatto: riga r, colonna c -> cells[r * cols + c]
    pub cells: Vec<Cell>,
    /// (riga, colonna)
    pub cursor: (usize, usize),
    pub cursor_visible: bool,
    /// Righe uscite dallo schermo, dalla più vecchia alla più recente.
    scrollback: VecDeque<Box<[Cell]>>,
    style: Style,
    saved_cursor: (usize, usize),
    wrap_pending: bool,
}

impl Term {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            cols,
            rows,
            cells: vec![Cell::default(); cols * rows],
            cursor: (0, 0),
            cursor_visible: true,
            scrollback: VecDeque::new(),
            style: Style::default(),
            saved_cursor: (0, 0),
            wrap_pending: false,
        }
    }

    pub fn row(&self, r: usize) -> &[Cell] {
        &self.cells[r * self.cols..(r + 1) * self.cols]
    }

    pub fn history_len(&self) -> usize {
        self.scrollback.len()
    }

    /// Riga del buffer combinato: 0 = più vecchia nello scrollback,
    /// history_len = prima riga dello schermo.
    pub fn line(&self, index: usize) -> &[Cell] {
        let history = self.scrollback.len();
        if index < history {
            &self.scrollback[index]
        } else if self.rows == 0 {
            &[]
        } else {
            let r = (index - history).min(self.rows - 1);
            self.row(r)
        }
    }

    /// Prima riga visibile quando si è risaliti di `scroll` righe.
    pub fn visible_top(&self, scroll: usize) -> usize {
        let history = self.scrollback.len();
        history - scroll.min(history)
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        let mut new = vec![Cell::default(); cols * rows];
        for r in 0..rows.min(self.rows) {
            for c in 0..cols.min(self.cols) {
                new[r * cols + c] = self.cells[r * self.cols + c];
            }
        }
        self.cells = new;
        self.cols = cols;
        self.rows = rows;
        self.cursor.0 = self.cursor.0.min(rows - 1);
        self.cursor.1 = self.cursor.1.min(cols - 1);
        self.wrap_pending = false;
    }

    fn goto(&mut self, row: usize, col: usize) {
        self.cursor = (row.min(self.rows - 1), col.min(self.cols - 1));
        self.wrap_pending = false;
    }

    fn linefeed(&mut self) {
        self.wrap_pending = false;
        if self.cursor.0 + 1 < self.rows {
            self.cursor.0 += 1;
        } else {
            self.scroll_up();
        }
    }

    fn scroll_up(&mut self) {
        let top: Box<[Cell]> = self.cells[..self.cols].into();
        self.scrollback.push_back(top);
        if self.scrollback.len() > MAX_SCROLLBACK {
            self.scrollback.pop_front();
        }
        self.cells.copy_within(self.cols.., 0);
        let start = (self.rows - 1) * self.cols;
        self.cells[start..].fill(Cell::default());
    }

    fn put(&mut self, ch: char) {
        if self.wrap_pending {
            self.wrap_pending = false;
            self.cursor.1 = 0;
            self.linefeed();
        }
        let (r, c) = self.cursor;
        self.cells[r * self.cols + c] = Cell {
            ch,
            style: self.style,
        };
        if c + 1 < self.cols {
            self.cursor.1 += 1;
        } else {
            self.wrap_pending = true;
        }
    }

    fn clear(&mut self, start: usize, end: usize) {
        self.cells[start..end].fill(Cell::default());
    }

    fn sgr(&mut self, params: &Params) {
        let flat: Vec<u16> = params.iter().flat_map(|p| p.iter().copied()).collect();
        if flat.is_empty() {
            self.style = Style::default();
            return;
        }

        let mut i = 0;
        while i < flat.len() {
            let p = flat[i];
            match p {
                0 => self.style = Style::default(),
                1 => self.style.bold = true,
                4 => self.style.underline = true,
                7 => self.style.inverse = true,
                22 => self.style.bold = false,
                24 => self.style.underline = false,
                27 => self.style.inverse = false,
                30..=37 => self.style.fg = Color::Indexed((p - 30) as u8),
                39 => self.style.fg = Color::Default,
                40..=47 => self.style.bg = Color::Indexed((p - 40) as u8),
                49 => self.style.bg = Color::Default,
                90..=97 => self.style.fg = Color::Indexed((p - 90 + 8) as u8),
                100..=107 => self.style.bg = Color::Indexed((p - 100 + 8) as u8),
                38 | 48 => {
                    let color = match flat.get(i + 1).copied() {
                        Some(5) => {
                            i += 2;
                            flat.get(i).map(|&n| Color::Indexed(n as u8))
                        }
                        Some(2) => {
                            let rgb = (
                                flat.get(i + 2).copied(),
                                flat.get(i + 3).copied(),
                                flat.get(i + 4).copied(),
                            );
                            i += 4;
                            match rgb {
                                (Some(r), Some(g), Some(b)) => {
                                    Some(Color::Rgb(r as u8, g as u8, b as u8))
                                }
                                _ => None,
                            }
                        }
                        _ => None,
                    };
                    if let Some(c) = color {
                        if p == 38 {
                            self.style.fg = c;
                        } else {
                            self.style.bg = c;
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
}

fn raw(params: &Params, i: usize) -> u16 {
    params
        .iter()
        .nth(i)
        .and_then(|p| p.first().copied())
        .unwrap_or(0)
}

/// Parametro con default quando è 0 o assente.
fn arg(params: &Params, i: usize, default: usize) -> usize {
    match raw(params, i) {
        0 => default,
        v => v as usize,
    }
}

impl Perform for Term {
    fn print(&mut self, c: char) {
        self.put(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x08 => {
                self.wrap_pending = false;
                self.cursor.1 = self.cursor.1.saturating_sub(1);
            }
            0x09 => {
                let c = ((self.cursor.1 / 8) + 1) * 8;
                self.goto(self.cursor.0, c);
            }
            0x0A..=0x0C => self.linefeed(),
            0x0D => {
                self.wrap_pending = false;
                self.cursor.1 = 0;
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        if !intermediates.is_empty() {
            // Modalità private: ESC[?25h / ESC[?25l per il cursore
            if intermediates.first() == Some(&b'?') && (action == 'h' || action == 'l') {
                for p in params.iter() {
                    if p.first() == Some(&25) {
                        self.cursor_visible = action == 'h';
                    }
                }
            }
            return;
        }

        let (r, c) = self.cursor;
        let cols = self.cols;
        let rows = self.rows;

        match action {
            'A' => self.goto(r.saturating_sub(arg(params, 0, 1)), c),
            'B' => self.goto(r + arg(params, 0, 1), c),
            'C' => self.goto(r, c + arg(params, 0, 1)),
            'D' => self.goto(r, c.saturating_sub(arg(params, 0, 1))),
            'G' => self.goto(r, arg(params, 0, 1) - 1),
            'd' => self.goto(arg(params, 0, 1) - 1, c),
            'H' | 'f' => self.goto(arg(params, 0, 1) - 1, arg(params, 1, 1) - 1),
            'J' => match raw(params, 0) {
                0 => self.clear(r * cols + c, rows * cols),
                1 => self.clear(0, r * cols + c + 1),
                _ => self.clear(0, rows * cols),
            },
            'K' => match raw(params, 0) {
                0 => self.clear(r * cols + c, (r + 1) * cols),
                1 => self.clear(r * cols, r * cols + c + 1),
                _ => self.clear(r * cols, (r + 1) * cols),
            },
            'm' => self.sgr(params),
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        if intermediates.is_empty() {
            match byte {
                b'7' => self.saved_cursor = self.cursor,
                b'8' => self.cursor = self.saved_cursor,
                _ => {}
            }
        }
    }
}

/// Emulatore completo: parser + stato del terminale.
pub struct Emulator {
    parser: Parser,
    pub term: Term,
}

impl Emulator {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self {
            parser: Parser::new(),
            term: Term::new(cols, rows),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.parser.advance(&mut self.term, byte);
        }
    }
}
