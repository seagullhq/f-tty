//! Screen grid: cells, cursor and scrollback.

use super::{Cell, Color, MAX_SCROLLBACK, Style};
use std::collections::VecDeque;
use vte::Params;

/// Main screen parked while the alternate screen (vim, less, htop) is shown.
struct Screen {
    cells: Vec<Cell>,
    wrapped: Vec<bool>,
    cursor: (usize, usize),
    saved_cursor: (usize, usize),
}

pub struct Term {
    pub cols: usize,
    pub rows: usize,
    /// Screen as a flat buffer: row r, column c -> cells[r * cols + c]
    pub cells: Vec<Cell>,
    /// (row, column)
    pub cursor: (usize, usize),
    pub cursor_visible: bool,
    /// Lines that left the screen, oldest to newest.
    scrollback: VecDeque<Box<[Cell]>>,
    /// Whether a line is a continuation of the previous one (soft wrap).
    /// Kept for scrollback and screen separately so resize can reflow text.
    scrollback_wrapped: VecDeque<bool>,
    wrapped: Vec<bool>,
    pub(super) style: Style,
    pub(super) saved_cursor: (usize, usize),
    pub(super) wrap_pending: bool,
    /// ConPTY chiede la posizione del cursore (ESC[6n) all'avvio e blocca
    /// la shell finche non arriva la risposta.
    pub dsr_pending: bool,
    /// Titolo impostato dalla shell/programma tramite OSC 0 / OSC 2.
    pub title: Option<String>,
    /// Cartella corrente comunicata tramite OSC 7 (URL `file://`).
    pub cwd: Option<String>,
    /// Scroll region (inclusive rows), set by DECSTBM.
    pub(super) top: usize,
    pub(super) bottom: usize,
    alt: Option<Screen>,
    /// DECCKM: arrow keys send `ESC O x` instead of `ESC [ x`.
    pub app_cursor: bool,
    /// Bracketed paste (2004).
    pub bracketed_paste: bool,
    /// Mouse reporting level: 0 off, else 1000 / 1002 / 1003.
    pub mouse: u16,
    /// SGR mouse encoding (1006).
    pub mouse_sgr: bool,
    /// BEL received since the flag was last taken.
    pub bell: bool,
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
            scrollback_wrapped: VecDeque::new(),
            wrapped: vec![false; rows],
            style: Style::default(),
            saved_cursor: (0, 0),
            wrap_pending: false,
            dsr_pending: false,
            title: None,
            cwd: None,
            top: 0,
            bottom: rows - 1,
            alt: None,
            app_cursor: false,
            bracketed_paste: false,
            mouse: 0,
            mouse_sgr: false,
            bell: false,
        }
    }

    pub fn row(&self, r: usize) -> &[Cell] {
        &self.cells[r * self.cols..(r + 1) * self.cols]
    }

    pub fn history_len(&self) -> usize {
        self.scrollback.len()
    }

    /// Appends a line to the history, dropping trailing blank cells: most
    /// lines are much shorter than the grid, and this is most of the tab's
    /// memory. The reader treats missing cells as blanks.
    fn push_history(&mut self, line: Box<[Cell]>, wrapped: bool) {
        self.scrollback.push_back(line);
        self.scrollback_wrapped.push_back(wrapped);
        if self.scrollback.len() > MAX_SCROLLBACK {
            self.scrollback.pop_front();
            self.scrollback_wrapped.pop_front();
        }
    }

    /// Combined buffer line: 0 = oldest in scrollback, history_len = first
    /// screen row.
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

    /// First visible line after scrolling back by `scroll` lines.
    pub fn visible_top(&self, scroll: usize) -> usize {
        let history = self.scrollback.len();
        history - scroll.min(history)
    }

    /// Etichetta della tab: titolo OSC, nome della cartella corrente (OSC 7)
    /// oppure il fallback passato dal chiamante.
    pub fn display_title(&self, fallback_id: usize) -> String {
        if let Some(title) = &self.title {
            let trimmed = title.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        if let Some(cwd) = &self.cwd {
            let name = cwd
                .rsplit(|c| c == '/' || c == '\\')
                .find(|part| !part.is_empty())
                .unwrap_or(cwd);
            if !name.is_empty() {
                return name.to_string();
            }
        }
        format!("Terminal {fallback_id}")
    }

    /// Resizes the grid, rewrapping existing text to the new column count.
    /// The font never changes: the window width changes how many columns are
    /// shown, so the text has to move with it.
    pub fn resize(&mut self, cols: usize, rows: usize) {
        // The alternate screen is blank-and-redrawn: apps repaint on SIGWINCH.
        let was_alt = self.alt.is_some();
        if was_alt {
            self.leave_alt();
        }
        self.resize_main(cols, rows);
        self.top = 0;
        self.bottom = self.rows - 1;
        if was_alt {
            self.enter_alt();
        }
    }

    pub fn in_alt_screen(&self) -> bool {
        self.alt.is_some()
    }

    pub(super) fn enter_alt(&mut self) {
        if self.alt.is_some() {
            return;
        }
        let blank = vec![Cell::default(); self.cols * self.rows];
        self.alt = Some(Screen {
            cells: std::mem::replace(&mut self.cells, blank),
            wrapped: std::mem::replace(&mut self.wrapped, vec![false; self.rows]),
            cursor: self.cursor,
            saved_cursor: self.saved_cursor,
        });
        self.top = 0;
        self.bottom = self.rows - 1;
    }

    pub(super) fn leave_alt(&mut self) {
        if let Some(main) = self.alt.take() {
            self.cells = main.cells;
            self.wrapped = main.wrapped;
            self.cursor = main.cursor;
            self.saved_cursor = main.saved_cursor;
            self.wrap_pending = false;
            self.top = 0;
            self.bottom = self.rows - 1;
        }
    }

    fn resize_main(&mut self, cols: usize, rows: usize) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        if cols == self.cols {
            self.resize_rows(rows);
            return;
        }

        let old_cols = self.cols;
        let history_len = self.scrollback.len();

        // All physical lines, oldest first, with their soft-wrap flags.
        let mut lines: Vec<Vec<Cell>> = self.scrollback.iter().map(|l| l.to_vec()).collect();
        let mut wrapped: Vec<bool> = self.scrollback_wrapped.iter().copied().collect();
        for r in 0..self.rows {
            lines.push(self.row(r).to_vec());
            wrapped.push(self.wrapped[r]);
        }
        if let Some(first) = wrapped.first_mut() {
            *first = false;
        }

        // Remember where the cursors were inside their logical lines so they
        // can be placed again after rewrapping.
        let cursor = logical_position(
            &wrapped,
            history_len + self.cursor.0,
            old_cols,
            self.cursor.1,
        );
        let saved = logical_position(
            &wrapped,
            history_len + self.saved_cursor.0,
            old_cols,
            self.saved_cursor.1,
        );

        // Join soft-wrapped lines, then split them at the new width. Explicit
        // line feeds start a new logical line and are never joined.
        let mut logical: Vec<Vec<Cell>> = Vec::new();
        let mut logical_start: Vec<usize> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if wrapped[i] && !logical.is_empty() {
                logical.last_mut().unwrap().extend_from_slice(line);
            } else {
                logical_start.push(i);
                logical.push(line.clone());
            }
        }
        for line in &mut logical {
            while line
                .last()
                .is_some_and(|c| c.ch == ' ' && c.style == Style::default())
            {
                line.pop();
            }
        }

        let mut new_lines: Vec<Vec<Cell>> = Vec::new();
        let mut new_wrapped: Vec<bool> = Vec::new();
        let mut new_logical_start: Vec<usize> = Vec::new();
        for line in logical {
            let start = new_lines.len();
            new_logical_start.push(start);
            let mut row = Vec::with_capacity(cols);
            for cell in line {
                row.push(cell);
                if row.len() == cols {
                    new_lines.push(std::mem::take(&mut row));
                    new_wrapped.push(new_lines.len() - 1 > start);
                }
            }
            if !row.is_empty() || new_lines.len() == start {
                row.resize(cols, Cell::default());
                new_lines.push(row);
                new_wrapped.push(new_lines.len() - 1 > start);
            }
        }

        // The tail stays on screen; everything above it becomes scrollback.
        // Blank rows at the end are unused screen space, not content: they
        // must not push real lines into the history when the grid changes.
        let content_end = new_lines
            .iter()
            .rposition(|line| line.iter().any(|cell| *cell != Cell::default()))
            .map_or(0, |i| i + 1);
        let split = content_end.saturating_sub(rows);
        self.scrollback.clear();
        self.scrollback_wrapped.clear();
        for (line, is_wrapped) in new_lines[..split].iter().zip(&new_wrapped[..split]) {
            self.push_history(trimmed(line), *is_wrapped);
        }

        let mut cells = Vec::with_capacity(rows * cols);
        for line in new_lines[split..].iter().take(rows) {
            cells.extend_from_slice(line);
        }
        while cells.len() < rows * cols {
            cells.extend(vec![Cell::default(); cols]);
        }
        cells.truncate(rows * cols);

        let cursor_phys = new_logical_start[cursor.0] + cursor.1 / cols;
        let saved_phys = new_logical_start[saved.0] + saved.1 / cols;
        let wrap_pending = self.wrap_pending;
        self.cells = cells;
        self.wrapped = new_wrapped[split..].iter().take(rows).copied().collect();
        self.wrapped.resize(rows, false);
        self.cols = cols;
        self.rows = rows;
        self.cursor = (
            cursor_phys.saturating_sub(split).min(rows - 1),
            (cursor.1 % cols).min(cols - 1),
        );
        self.saved_cursor = (
            saved_phys.saturating_sub(split).min(rows - 1),
            (saved.1 % cols).min(cols - 1),
        );
        self.wrap_pending = wrap_pending && self.cursor.1 + 1 == cols;
    }

    /// Only the row count changed: keep the existing lines as they are.
    fn resize_rows(&mut self, rows: usize) {
        let mut cells = vec![Cell::default(); self.cols * rows];
        for r in 0..rows.min(self.rows) {
            let start = r * self.cols;
            cells[start..start + self.cols].copy_from_slice(self.row(r));
        }
        let mut wrapped = vec![false; rows];
        wrapped[..rows.min(self.rows)].copy_from_slice(&self.wrapped[..rows.min(self.rows)]);
        self.cells = cells;
        self.wrapped = wrapped;
        self.rows = rows;
        self.cursor.0 = self.cursor.0.min(rows - 1);
        self.cursor.1 = self.cursor.1.min(self.cols - 1);
        self.saved_cursor.0 = self.saved_cursor.0.min(rows - 1);
        self.saved_cursor.1 = self.saved_cursor.1.min(self.cols - 1);
        self.wrap_pending = false;
    }

    pub(super) fn goto(&mut self, row: usize, col: usize) {
        self.cursor = (row.min(self.rows - 1), col.min(self.cols - 1));
        self.wrap_pending = false;
    }

    pub(super) fn linefeed(&mut self, soft_wrap: bool) {
        self.wrap_pending = false;
        if self.cursor.0 == self.bottom {
            self.scroll_region_up(self.top, self.bottom, 1);
        } else if self.cursor.0 + 1 < self.rows {
            self.cursor.0 += 1;
        }
        self.wrapped[self.cursor.0] = soft_wrap;
    }

    /// Reverse index: cursor up, scrolling the region down at its top.
    pub(super) fn reverse_index(&mut self) {
        self.wrap_pending = false;
        if self.cursor.0 == self.top {
            self.scroll_region_down(self.top, self.bottom, 1);
        } else {
            self.cursor.0 = self.cursor.0.saturating_sub(1);
        }
    }

    /// Scrolls rows `top..=bottom` up. Lines leaving the top of the main
    /// screen go to the scrollback; region and alternate-screen scrolls don't.
    pub(super) fn scroll_region_up(&mut self, top: usize, bottom: usize, n: usize) {
        let cols = self.cols;
        for _ in 0..n.min(bottom + 1 - top) {
            if top == 0 && self.alt.is_none() {
                let line = trimmed(&self.cells[..cols]);
                let wrapped = self.wrapped[0];
                self.push_history(line, wrapped);
            }
            self.cells
                .copy_within((top + 1) * cols..(bottom + 1) * cols, top * cols);
            self.wrapped.copy_within(top + 1..=bottom, top);
            self.cells[bottom * cols..(bottom + 1) * cols].fill(Cell::default());
            self.wrapped[bottom] = false;
        }
    }

    pub(super) fn scroll_region_down(&mut self, top: usize, bottom: usize, n: usize) {
        let cols = self.cols;
        for _ in 0..n.min(bottom + 1 - top) {
            self.cells
                .copy_within(top * cols..bottom * cols, (top + 1) * cols);
            self.wrapped.copy_within(top..bottom, top + 1);
            self.cells[top * cols..(top + 1) * cols].fill(Cell::default());
            self.wrapped[top] = false;
        }
    }

    /// DECSET / DECRST.
    pub(super) fn set_private_mode(&mut self, mode: u16, on: bool) {
        match mode {
            1 => self.app_cursor = on,
            25 => self.cursor_visible = on,
            47 | 1047 | 1049 => {
                if on {
                    self.enter_alt();
                } else {
                    self.leave_alt();
                }
            }
            1000 | 1002 | 1003 => {
                if on {
                    self.mouse = mode;
                } else if self.mouse == mode {
                    self.mouse = 0;
                }
            }
            1006 => self.mouse_sgr = on,
            2004 => self.bracketed_paste = on,
            _ => {}
        }
    }

    pub(super) fn set_region(&mut self, top: usize, bottom: usize) {
        let bottom = bottom.min(self.rows - 1);
        if top < bottom {
            self.top = top;
            self.bottom = bottom;
        }
        self.goto(0, 0);
    }

    pub(super) fn insert_lines(&mut self, n: usize) {
        if (self.top..=self.bottom).contains(&self.cursor.0) {
            self.scroll_region_down(self.cursor.0, self.bottom, n);
            self.cursor.1 = 0;
            self.wrap_pending = false;
        }
    }

    pub(super) fn delete_lines(&mut self, n: usize) {
        if (self.top..=self.bottom).contains(&self.cursor.0) {
            // Deleted lines are discarded, not sent to the scrollback.
            let cols = self.cols;
            let (r, bottom) = (self.cursor.0, self.bottom);
            for _ in 0..n.min(bottom + 1 - r) {
                self.cells
                    .copy_within((r + 1) * cols..(bottom + 1) * cols, r * cols);
                self.wrapped.copy_within(r + 1..=bottom, r);
                self.cells[bottom * cols..(bottom + 1) * cols].fill(Cell::default());
                self.wrapped[bottom] = false;
            }
            self.cursor.1 = 0;
            self.wrap_pending = false;
        }
    }

    /// ICH: shifts the rest of the row right, opening `n` blanks at the cursor.
    pub(super) fn insert_chars(&mut self, n: usize) {
        let (r, c) = self.cursor;
        let row = &mut self.cells[r * self.cols..(r + 1) * self.cols];
        let n = n.min(self.cols - c);
        row.copy_within(c..self.cols - n, c + n);
        row[c..c + n].fill(Cell::default());
        self.wrap_pending = false;
    }

    /// DCH: removes `n` cells at the cursor, pulling the rest of the row left.
    pub(super) fn delete_chars(&mut self, n: usize) {
        let (r, c) = self.cursor;
        let row = &mut self.cells[r * self.cols..(r + 1) * self.cols];
        let n = n.min(self.cols - c);
        row.copy_within(c + n.., c);
        row[self.cols - n..].fill(Cell::default());
        self.wrap_pending = false;
    }

    /// ECH: blanks `n` cells from the cursor without moving anything.
    pub(super) fn erase_chars(&mut self, n: usize) {
        let (r, c) = self.cursor;
        let start = r * self.cols + c;
        self.clear(start, start + n.min(self.cols - c));
    }

    pub(super) fn put(&mut self, ch: char) {
        if self.wrap_pending {
            self.cursor.1 = 0;
            self.linefeed(true);
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

    pub(super) fn clear(&mut self, start: usize, end: usize) {
        self.cells[start..end].fill(Cell::default());
    }

    pub(super) fn sgr(&mut self, params: &Params) {
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

/// A history line without its trailing blank cells.
fn trimmed(cells: &[Cell]) -> Box<[Cell]> {
    let end = cells
        .iter()
        .rposition(|cell| *cell != Cell::default())
        .map_or(0, |i| i + 1);
    cells[..end].into()
}

/// Position of a physical line inside its logical (soft-wrapped) line:
/// (logical line index, character offset from its start).
fn logical_position(wrapped: &[bool], line: usize, cols: usize, col: usize) -> (usize, usize) {
    let mut start = line.min(wrapped.len().saturating_sub(1));
    while start > 0 && wrapped[start] {
        start -= 1;
    }
    let logical = wrapped[..start].iter().filter(|&&w| !w).count();
    (logical, (line - start) * cols + col)
}
