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
        let Some(code) = params.first().copied() else {
            return;
        };
        let Some(value) = params.get(1) else {
            return;
        };
        match code {
            // OSC 0 imposta icona e titolo; con due valori il titolo è l'ultimo.
            b"0" | b"2" => {
                let text = String::from_utf8_lossy(if code == b"0" {
                    params.last().copied().unwrap_or(value)
                } else {
                    value
                })
                .trim()
                .to_string();
                self.title = if text.is_empty() { None } else { Some(text) };
            }
            b"7" => {
                self.cwd = cwd_from_url(&String::from_utf8_lossy(value));
            }
            // OSC 9: desktop notification text from the program.
            b"9" => self.notice = Some(String::from_utf8_lossy(value).into_owned()),
            b"133" => match value.first() {
                Some(b'A') => self.mark_next = true,
                Some(b'D') => self.done = true,
                _ => {}
            },
            // OSC 52 ; targets ; base64 — write only. Reading the clipboard
            // back would let any program in the tab spy on it.
            b"52" => {
                if let Some(text) = params.get(2).and_then(|data| base64_decode(data)) {
                    self.clipboard = String::from_utf8(text).ok();
                }
            }
            // OSC 8 ; params ; uri  (the uri may itself contain ';')
            b"8" => {
                let uri = params.get(2..).unwrap_or_default().join(&b';');
                self.set_link(&String::from_utf8_lossy(&uri));
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

/// Largest OSC 52 payload accepted, as encoded bytes (about 750 KB of text).
const MAX_CLIPBOARD: usize = 1 << 20;

/// Standard-alphabet base64, tolerant of whitespace and missing padding.
/// `None` on any other byte or an oversized payload.
fn base64_decode(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() > MAX_CLIPBOARD {
        return None;
    }
    let (mut out, mut acc, mut bits) = (Vec::with_capacity(data.len() * 3 / 4), 0u32, 0);
    for &b in data {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b' ' | b'\r' | b'\n' => continue,
            _ => return None,
        };
        acc = acc << 6 | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}
