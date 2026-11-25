//! Clickable links: OSC 8 hyperlinks and plain `http(s)://` URLs, opened
//! with Ctrl+click.

use crate::app::TerminalView;
use crate::terminal::Cell;
use gpui::*;

/// Only schemes that are safe to hand to the system: web pages, mail, and
/// files on this machine (a `file://` naming another host, as over SSH, is
/// not ours to open).
pub(crate) fn openable(uri: &str) -> bool {
    let lower = uri.to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("file:///")
        || lower.starts_with("file://localhost/")
}

/// `http(s)://` URL covering column `col`, found by text alone.
pub(crate) fn plain_url(cells: &[Cell], col: usize) -> Option<String> {
    let blank = |i: usize| cells[i].ch.is_whitespace();
    if col >= cells.len() || blank(col) {
        return None;
    }
    let mut start = col;
    while start > 0 && !blank(start - 1) {
        start -= 1;
    }
    let mut end = col + 1;
    while end < cells.len() && !blank(end) {
        end += 1;
    }
    let text: String = cells[start..end].iter().map(|c| c.ch).collect();
    // Brackets and quotes around the URL are not part of it.
    let inner = text.trim_start_matches(['(', '[', '{', '<', '\'', '"']);
    let start = start + text.chars().count() - inner.chars().count();
    let url = inner.trim_end_matches(['.', ',', ';', ':', ')', ']', '}', '>', '\'', '"', '!', '?']);
    let lower = url.to_ascii_lowercase();
    let valid = lower.starts_with("http://") || lower.starts_with("https://");
    (valid && (start..start + url.chars().count()).contains(&col)).then(|| url.to_string())
}

impl TerminalView {
    /// Link under a window position: the cell's OSC 8 target, else a URL
    /// written in the text.
    pub(crate) fn link_at(&self, position: Point<Pixels>) -> Option<String> {
        let tab = self.tabs.get(self.active)?;
        let term = &tab.emu.term;
        let (row, col) = self.cell_at(position);
        let cells = term.line(term.visible_top(tab.scroll) + row);
        let uri = match cells.get(col).and_then(|cell| term.link_of(cell)) {
            Some(uri) => uri.to_string(),
            None => plain_url(cells, col)?,
        };
        openable(&uri).then_some(uri)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(text: &str) -> Vec<Cell> {
        text.chars()
            .map(|ch| Cell {
                ch,
                ..Cell::default()
            })
            .collect()
    }

    #[::core::prelude::v1::test]
    fn plain_url_trims_punctuation_and_needs_the_click_on_it() {
        let c = cells("see (https://a.io/x?y=1), ok");
        assert_eq!(plain_url(&c, 8).as_deref(), Some("https://a.io/x?y=1"));
        assert_eq!(plain_url(&c, 0), None); // on "see"
        assert_eq!(plain_url(&c, 4), None); // on the "(" before the scheme
        assert_eq!(plain_url(&cells("ftp://x"), 2), None);
    }

    #[::core::prelude::v1::test]
    fn openable_blocks_remote_files_and_odd_schemes() {
        assert!(openable("https://a.io"));
        assert!(openable("file:///home/me/a.txt"));
        assert!(!openable("file://other-host/etc/passwd"));
        assert!(!openable("javascript:alert(1)"));
        assert!(!openable("ssh://host"));
    }

    #[::core::prelude::v1::test]
    fn osc8_marks_cells_and_ends() {
        let mut emu = crate::terminal::Emulator::new(20, 2);
        emu.feed(b"\x1b]8;;https://a.io/p;q\x07hi\x1b]8;;\x07 x");
        let term = &emu.term;
        let line = term.line(0);
        assert_eq!(term.link_of(&line[0]), Some("https://a.io/p;q"));
        assert_eq!(term.link_of(&line[1]), Some("https://a.io/p;q"));
        assert_eq!(term.link_of(&line[3]), None);
    }
}
