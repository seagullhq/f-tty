//! VT parser glue: `vte::Perform` implementation for `Term`.

use super::Term;
use vte::{Params, Parser, Perform};

fn raw(params: &Params, i: usize) -> u16 {
    params
        .iter()
        .nth(i)
        .and_then(|p| p.first().copied())
        .unwrap_or(0)
}

/// Parameter with a default when 0 or missing.
fn arg(params: &Params, i: usize, default: usize) -> usize {
    match raw(params, i) {
        0 => default,
        v => v as usize,
    }
}

/// Decodifica `%XX` in un URL `file://`.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_value(bytes[i + 1]), hex_value(bytes[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Estrae il percorso da un URL OSC 7 (`file://host/path` o `file:///path`).
fn cwd_from_url(value: &str) -> Option<String> {
    let rest = value.trim().strip_prefix("file://")?;
    let path = match rest.find('/') {
        Some(i) => &rest[i..],
        None => return None,
    };
    let decoded = percent_decode(path);
    // Su Windows `file:///C:/...` ha uno slash iniziale prima dell'unità.
    #[cfg(windows)]
    let decoded = {
        let bytes = decoded.as_bytes();
        if bytes.len() >= 3 && bytes[0] == b'/' && bytes[2] == b':' {
            decoded[1..].to_string()
        } else {
            decoded
        }
    };
    if decoded.is_empty() {
        None
    } else {
        Some(decoded)
    }
}

impl Perform for Term {
    fn print(&mut self, c: char) {
        self.put(c);
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        let Some(code) = params.first().and_then(|p| p.first()).copied() else {
            return;
        };
        let Some(value) = params.get(1) else {
            return;
        };
        match code {
            // OSC 0 imposta icona e titolo; con due valori il titolo è l'ultimo.
            b'0' | b'2' => {
                let text = String::from_utf8_lossy(if code == b'0' {
                    params.last().copied().unwrap_or(value)
                } else {
                    value
                })
                .trim()
                .to_string();
                self.title = if text.is_empty() { None } else { Some(text) };
            }
            b'7' => {
                self.cwd = cwd_from_url(&String::from_utf8_lossy(value));
            }
            _ => {}
        }
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => self.bell = true,
            0x08 => {
                self.wrap_pending = false;
                self.cursor.1 = self.cursor.1.saturating_sub(1);
            }
            0x09 => {
                let c = ((self.cursor.1 / 8) + 1) * 8;
                self.goto(self.cursor.0, c);
            }
            0x0A..=0x0C => self.linefeed(false),
            0x0D => {
                self.wrap_pending = false;
                self.cursor.1 = 0;
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        if !intermediates.is_empty() {
            if intermediates.first() == Some(&b'?') && (action == 'h' || action == 'l') {
                let on = action == 'h';
                for p in params.iter() {
                    self.set_private_mode(p.first().copied().unwrap_or(0), on);
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
            'E' => self.goto(r + arg(params, 0, 1), 0),
            'F' => self.goto(r.saturating_sub(arg(params, 0, 1)), 0),
            'L' => self.insert_lines(arg(params, 0, 1)),
            'M' => self.delete_lines(arg(params, 0, 1)),
            '@' => self.insert_chars(arg(params, 0, 1)),
            'P' => self.delete_chars(arg(params, 0, 1)),
            'X' => self.erase_chars(arg(params, 0, 1)),
            'S' => self.scroll_region_up(self.top, self.bottom, arg(params, 0, 1)),
            'T' => self.scroll_region_down(self.top, self.bottom, arg(params, 0, 1)),
            'r' => self.set_region(arg(params, 0, 1) - 1, arg(params, 1, rows) - 1),
            's' => self.saved_cursor = self.cursor,
            'u' => self.cursor = self.saved_cursor,
            'n' if raw(params, 0) == 6 => self.dsr_pending = true,
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        if intermediates.is_empty() {
            match byte {
                b'7' => self.saved_cursor = self.cursor,
                b'8' => self.cursor = self.saved_cursor,
                b'D' => self.linefeed(false),
                b'E' => {
                    self.cursor.1 = 0;
                    self.linefeed(false);
                }
                b'M' => self.reverse_index(),
                _ => {}
            }
        }
    }
}

/// Full emulator: parser + terminal state.
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
