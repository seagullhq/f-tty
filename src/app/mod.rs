//! Application state and behavior: tabs, input, selection, layout, settings and startup tuning.

mod input;
mod layout;
mod links;
mod notify;
mod panes;
mod prompts;
mod scrollbar;
mod search;
mod selection;
mod settings;
mod tree;
pub(crate) use settings::config_dir;
pub(crate) mod startup;

use crate::terminal::Emulator;
use crate::terminal::pty::Pty;
use crate::terminal::shells::{self, Shell};
use crate::ui::components::Combobox;
use crate::ui::theme;
use crate::{COLS, ROWS};
use gpui::*;
use std::ops::{Deref, DerefMut};
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};
use tree::Tree;

/// A finished command pops a notification only if it ran at least this long.
const NOTIFY_AFTER: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabPosition {
    Top,
    Bottom,
    Left,
    Right,
}

impl TabPosition {
    pub(crate) const ALL: [Self; 4] = [Self::Top, Self::Bottom, Self::Left, Self::Right];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Bottom => "Bottom",
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|position| position.label() == value)
    }
}

impl Default for TabPosition {
    fn default() -> Self {
        Self::Top
    }
}

/// One terminal session inside a tab: shell, screen and scroll state.
pub(crate) struct Pane {
    pub(crate) id: usize,
    pub(crate) pty: Option<Pty>,
    pub(crate) emu: Emulator,
    /// Changes on every grid resize: PTY resizes only start when the drag
    /// stops (the latest generation wins).
    pub(crate) resize_seq: u64,
    /// Scrollback lines shown above the screen (0 = bottom).
    pub(crate) scroll: usize,
    /// Accumulator for fractional touchpad deltas.
    pub(crate) scroll_acc: f32,
    /// True while the user has typed input that has not been submitted yet.
    /// Prevents the resize redraw from executing a pending command.
    pub(crate) input_pending: bool,
    /// When the running command was submitted (Enter), until it finishes.
    pub(crate) busy_since: Option<Instant>,
    /// Last size applied to the PTY, used to detect column changes.
    pub(crate) pty_size: (u16, u16),
    /// Window position of the pane's first cell and its size, set on render.
    pub(crate) origin: Point<Pixels>,
    pub(crate) area: Size<Pixels>,
    pub(crate) born: Instant,
    /// The shell ended; the pane is removed on the next pump.
    pub(crate) exited: bool,
}

impl Pane {
    fn spawn(id: usize, shell: &Shell, cwd: Option<&str>) -> Self {
        let mut emu = Emulator::new(COLS, ROWS);
        let pty = match Pty::spawn(COLS as u16, ROWS as u16, &shell.program, cwd) {
            Ok(pty) => Some(pty),
            Err(err) => {
                let msg = format!(
                    "Impossibile avviare la shell \"{}\" ({})\r\n\r\n{}\r\n",
                    shell.name, shell.program, err
                );
                emu.feed(msg.as_bytes());
                None
            }
        };
        Self {
            id,
            pty,
            emu,
            resize_seq: 0,
            scroll: 0,
            scroll_acc: 0.,
            input_pending: false,
            busy_since: None,
            pty_size: (COLS as u16, ROWS as u16),
            origin: point(px(0.), px(0.)),
            area: size(px(1.), px(1.)),
            born: Instant::now(),
            exited: false,
        }
    }
}

/// Most panes one tab can hold: beyond this they become too small to use.
pub(crate) const MAX_PANES: usize = 8;

pub(crate) struct Tab {
    pub(crate) id: usize,
    pub(crate) title: String,
    /// Shell tag shown in the tab (see `Shell::badge`).
    pub(crate) badge: String,
    /// Accent color of the badge, unique among open tabs while possible.
    pub(crate) tint: u32,
    /// A bell rang while the tab was out of sight.
    pub(crate) attention: bool,
    /// Never empty, in creation order.
    pub(crate) panes: Vec<Pane>,
    /// Index of the focused pane in `panes`.
    pub(crate) focus: usize,
    /// How the panes are arranged (leaves are pane ids).
    pub(crate) tree: Tree,
    /// Area the tree fills, set on render (window coordinates).
    pub(crate) rect: tree::Rect,
    /// Only the focused pane is shown, filling the area.
    pub(crate) zoomed: bool,
    /// Typed input goes to every pane of the tab.
    pub(crate) broadcast: bool,
}

/// The tab behaves as its focused pane for everything about a single
/// terminal (`tab.emu`, `tab.scroll`, `tab.pty`, ...).
impl Deref for Tab {
    type Target = Pane;
    fn deref(&self) -> &Pane {
        &self.panes[self.focus]
    }
}

impl DerefMut for Tab {
    fn deref_mut(&mut self) -> &mut Pane {
        &mut self.panes[self.focus]
    }
}

impl Tab {
    fn terminal(id: usize, shell: &Shell, tint: u32, cwd: Option<&str>) -> Self {
        Self {
            id,
            title: format!("Terminal {id}"),
            badge: shell.badge(),
            tint,
            attention: false,
            panes: vec![Pane::spawn(id, shell, cwd)],
            focus: 0,
            tree: Tree::Leaf(id),
            rect: (0., 0., 1., 1.),
            zoomed: false,
            broadcast: false,
        }
    }

    pub(crate) fn focus_id(&self) -> usize {
        self.panes[self.focus].id
    }

    pub(crate) fn index_of(&self, id: usize) -> Option<usize> {
        self.panes.iter().position(|p| p.id == id)
    }

    /// Opens `pane` next to the focused one (right or below, by `axis`) and
    /// focuses it. False if the tree could not take it.
    fn add_pane(&mut self, pane: Pane, axis: Axis) -> bool {
        if !self.tree.split_leaf(self.focus_id(), pane.id, axis) {
            return false;
        }
        self.panes.push(pane);
        self.focus = self.panes.len() - 1;
        self.zoomed = false;
        true
    }

    /// Removes pane `i` (never the last one). If it had focus, focus moves
    /// to the previous pane in tree order, else the next.
    fn remove_pane(&mut self, i: usize) {
        let (id, focused) = (self.panes[i].id, self.focus_id());
        let order = self.tree.leaves();
        let at = order.iter().position(|l| *l == id).unwrap_or(0);
        let heir = if at > 0 {
            order.get(at - 1)
        } else {
            order.get(at + 1)
        };
        let keep = if id == focused {
            heir.copied().unwrap_or(focused)
        } else {
            focused
        };
        self.tree.remove_leaf(id);
        self.panes.remove(i);
        self.focus = self.index_of(keep).unwrap_or(0);
        self.zoomed = false;
    }
}

/// Removes panes whose shell ended, and tabs whose only pane did.
/// True if anything was removed.
fn prune_exited(tabs: &mut Vec<Tab>) -> bool {
    let before: usize = tabs.iter().map(|t| t.panes.len()).sum();
    for tab in tabs.iter_mut() {
        while tab.panes.len() > 1 {
            match tab.panes.iter().position(|p| p.exited) {
                Some(i) => tab.remove_pane(i),
                None => break,
            }
        }
    }
    tabs.retain(|t| !(t.panes.len() == 1 && t.panes[0].exited));
    before != tabs.iter().map(|t| t.panes.len()).sum::<usize>()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Selection {
    pub(crate) anchor: (usize, usize),
    pub(crate) cursor: (usize, usize),
}

pub(crate) struct TerminalView {
    pub(crate) focus: FocusHandle,
    pub(crate) tabs: Vec<Tab>,
    pub(crate) active: usize,
    pub(crate) next_id: usize,
    pub(crate) tabs_position: TabPosition,
    /// Width of the side tab list, changed by dragging its inner edge.
    pub(crate) strip_width: Pixels,
    /// Pointer x and strip width when a drag of the edge started.
    pub(crate) strip_drag: Option<(Pixels, Pixels)>,
    pub(crate) shells: Vec<Shell>,
    pub(crate) shell: Shell,
    pub(crate) settings_open: bool,
    pub(crate) cell_width: Pixels,
    pub(crate) line_height: Pixels,
    pub(crate) selection: Option<Selection>,
    pub(crate) selecting: bool,
    /// A divider between panes is being dragged.
    pub(crate) split_drag: Option<panes::SplitDrag>,
    /// Every shell ended: the window closes on the next render.
    pub(crate) quit: bool,
    /// The scrollbar thumb is being dragged.
    pub(crate) scrollbar_drag: bool,
    /// Scrollback search bar, when open.
    pub(crate) search: Option<search::Search>,
    /// A left press was sent to the program, so its drag and release are too.
    pub(crate) mouse_reported: bool,
    /// Family used by the terminal grid; changeable from settings.
    pub(crate) font: SharedString,
    pub(crate) fonts: Vec<String>,
    pub(crate) font_picker: Combobox,
}

impl TerminalView {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        window.set_window_title("ftty");

        let saved = settings::load();
        let shells = shells::available();
        let shell = if saved.shell.is_empty() {
            None
        } else {
            shells
                .iter()
                .find(|shell| shell.program == saved.shell)
                .cloned()
        }
        .or_else(|| shells::default_shell(&shells))
        .expect("no shell available");

        // Single timer for pumping output: no blocked UI threads.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(8))
                    .await;
                if this.update(cx, |view, cx| view.pump(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();

        let fonts = window.text_system().all_font_names();
        let font_family = if saved.font.is_empty() || !fonts.contains(&saved.font) {
            crate::FONT.to_string()
        } else {
            saved.font.clone()
        };
        let font_size = window.rem_size() * 0.875;
        let cell_width = Self::measure_cell_width(font(font_family.clone()), font_size, window);
        // TextStyle::default() uses phi() = 1.618034.
        let line_height = font_size * 1.618034;

        Self {
            focus,
            tabs: vec![Tab::terminal(1, &shell, layout::pick_tint(&[]), None)],
            active: 0,
            next_id: 2,
            tabs_position: saved.tabs_position,
            strip_width: px(saved.strip)
                .max(crate::ui::theme::STRIP_MIN)
                .min(crate::ui::theme::STRIP_MAX),
            strip_drag: None,
            shells,
            shell,
            settings_open: false,
            cell_width,
            line_height,
            selection: None,
            selecting: false,
            split_drag: None,
            quit: false,
            scrollbar_drag: false,
            search: None,
            mouse_reported: false,
            font: font_family.into(),
            fonts,
            font_picker: Combobox::new("Search fonts…", cx),
        }
    }

    /// Width of one grid cell for the given family; fall back to 1px when the
    /// text system cannot shape the font (e.g. an unavailable family).
    fn measure_cell_width(family: Font, font_size: Pixels, window: &Window) -> Pixels {
        let run = TextRun {
            len: 1,
            font: family,
            color: black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let width = window
            .text_system()
            .shape_line("M".into(), font_size, &[run], None)
            .width;
        if width > px(0.) { width } else { px(1.) }
    }

    /// Writes the current choices to the user config file (best effort).
    pub(crate) fn save_settings(&self) {
        settings::save(&settings::SavedSettings {
            tabs_position: self.tabs_position,
            shell: self.shell.program.clone(),
            font: self.font.to_string(),
            strip: self.strip_width.into(),
        });
    }

    /// Opens or closes the font dropdown, moving focus into or out of it.
    pub(crate) fn toggle_font_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.font_picker.is_open() {
            self.font_picker.close();
            window.focus(&self.focus, cx);
        } else {
            self.font_picker.open(&self.font, &self.fonts, cx);
            let handle = self.font_picker.focus_handle(cx);
            window.focus(&handle, cx);
        }
        cx.notify();
    }

    /// Applies a font family to new renders and re-aligns the grid to it.
    pub(crate) fn select_font(
        &mut self,
        family: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.font_picker.close();
        window.focus(&self.focus, cx);
        if self.font == family {
            cx.notify();
            return;
        }
        self.font = family.clone().into();
        let font_size = window.rem_size() * 0.875;
        self.cell_width = Self::measure_cell_width(font(family), font_size, window);
        self.selection = None;
        self.save_settings();
        cx.notify();
    }

    /// Changes the tab list side. The window keeps its size: the terminal
    /// absorbs the bar, except when the window falls under the new minimum.
    pub(crate) fn set_tab_position(
        &mut self,
        position: TabPosition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection = None;
        self.selecting = false;
        if position != self.tabs_position {
            self.tabs_position = position;
            if !window.is_maximized() && !window.is_fullscreen() {
                let current = window.bounds().size;
                let min = layout::min_size(position);
                if current.width < min.width || current.height < min.height {
                    window.resize(size(
                        current.width.max(min.width),
                        current.height.max(min.height),
                    ));
                }
            }
            self.save_settings();
        }
        cx.notify();
    }

    /// Drains chunks from all panes and notifies on output or title changes.
    fn pump(&mut self, cx: &mut Context<Self>) {
        let mut active_received = false;
        let mut title_changed = false;
        let window_away = cx.active_window().is_none();
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            let hidden = i != self.active || self.settings_open;
            let away = hidden || window_away;
            let Tab {
                panes,
                focus,
                id,
                title,
                attention,
                ..
            } = tab;
            let mut received = false;
            for (j, pane) in panes.iter_mut().enumerate() {
                let history_before = pane.emu.term.history_len();
                let mut dsr: Option<Vec<u8>> = None;
                let mut gone = false;
                if let Some(pty) = &pane.pty {
                    loop {
                        match pty.rx.try_recv() {
                            Ok(chunk) => {
                                pane.emu.feed(&chunk);
                                if pane.emu.term.dsr_pending {
                                    pane.emu.term.dsr_pending = false;
                                    let (row, col) = pane.emu.term.cursor;
                                    dsr =
                                        Some(format!("\x1b[{};{}R", row + 1, col + 1).into_bytes());
                                }
                                received = true;
                            }
                            Err(TryRecvError::Empty) => break,
                            Err(TryRecvError::Disconnected) => {
                                gone = true;
                                break;
                            }
                        }
                    }
                }
                // ConPTY keeps its output pipe open after the shell exits, so
                // EOF alone never arrives on Windows. Poll the child process.
                if !gone {
                    if let Some(pty) = &mut pane.pty {
                        if pty.exit_status().is_some() {
                            gone = true;
                        }
                    }
                }
                if gone {
                    // Only a quick *failed* start stays on screen so its error
                    // can be read. A clean `exit` closes even if typed at once.
                    let quick_failure = pane
                        .pty
                        .as_mut()
                        .and_then(|pty| pty.exit_status())
                        .is_some_and(|status| !status.success())
                        && pane.born.elapsed() < Duration::from_secs(2);
                    pane.pty = None;
                    received = true;
                    if quick_failure {
                        pane.emu.feed(b"\r\n[process exited]\r\n");
                    } else {
                        pane.exited = true;
                    }
                }
                // Rispondi al DSR fuori dal borrow dell'emulatore: altrimenti
                // ConPTY resta in attesa e la shell su Windows non parte.
                if let Some(response) = dsr {
                    if let Some(pty) = &mut pane.pty {
                        let _ = pty.write(&response);
                    }
                }
                if std::mem::take(&mut pane.emu.term.bell) && hidden {
                    *attention = true;
                    received = true;
                }
                if let Some(text) = pane.emu.term.clipboard.take() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
                // Notifications: shown when the user is not looking at this tab.
                if let Some(text) = pane.emu.term.notice.take() {
                    if away {
                        *attention = hidden;
                        received = true;
                        notify::send(title, &text);
                    }
                }
                if std::mem::take(&mut pane.emu.term.done) {
                    // Only commands submitted with Enter count: the prompt that
                    // follows shell startup also reports "done".
                    if let Some(since) = pane.busy_since.take() {
                        if hidden {
                            *attention = true;
                            received = true;
                        }
                        if away && since.elapsed() >= NOTIFY_AFTER {
                            notify::send(title, "Command finished");
                        }
                    }
                }
                let added = pane.emu.term.history_len() - history_before;
                if pane.scroll > 0 && added > 0 {
                    // The text in view stays put while output arrives.
                    pane.scroll = (pane.scroll + added).min(pane.emu.term.history_len());
                }
                if j == *focus {
                    let new_title = pane.emu.term.display_title(*id);
                    if new_title != *title {
                        *title = new_title;
                        title_changed = true;
                    }
                }
            }
            if !hidden {
                *attention = false;
            }
            if received && i == self.active {
                active_received = true;
            }
        }
        let closed = self.close_exited();
        if active_received || title_changed || closed {
            cx.notify();
        }
    }

    /// Drops panes whose shell ended, then tabs left empty; the window
    /// closes when nothing is left. True if anything changed.
    fn close_exited(&mut self) -> bool {
        let active_id = self.tabs.get(self.active).map(|t| t.id);
        if !prune_exited(&mut self.tabs) {
            return false;
        }
        self.quit = self.tabs.is_empty();
        self.active = self
            .tabs
            .iter()
            .position(|t| Some(t.id) == active_id)
            .unwrap_or_else(|| self.active.min(self.tabs.len().saturating_sub(1)));
        self.selection = None;
        self.selecting = false;
        true
    }

    pub(crate) fn open_tab(&mut self, cx: &mut Context<Self>) {
        let used: Vec<u32> = self.tabs.iter().map(|t| t.tint).collect();
        let cwd = self
            .tabs
            .get(self.active)
            .and_then(|t| t.emu.term.cwd.clone());
        self.tabs.push(Tab::terminal(
            self.next_id,
            &self.shell,
            layout::pick_tint(&used),
            cwd.as_deref(),
        ));
        self.next_id += 1;
        self.active = self.tabs.len() - 1;
        self.settings_open = false;
        self.selection = None;
        self.selecting = false;
        cx.notify();
    }

    /// Shows/hides settings in the terminal area.
    pub(crate) fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_open = !self.settings_open;
        self.selection = None;
        self.selecting = false;
        cx.notify();
    }

    /// Sets the shell used by new tabs.
    pub(crate) fn select_shell(&mut self, shell: Shell, cx: &mut Context<Self>) {
        self.shell = shell;
        self.save_settings();
        cx.notify();
    }

    pub(crate) fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        self.tabs.remove(index);
        startup::release_free_pages();
        self.selection = None;
        self.selecting = false;
        if self.tabs.is_empty() {
            window.remove_window();
            return;
        }
        self.active = layout::active_after_close(self.active, index, self.tabs.len());
        cx.notify();
    }

    pub(crate) fn cycle_tab(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.tabs.len() < 2 {
            return;
        }
        self.active = (self.active as isize + delta).rem_euclid(self.tabs.len() as isize) as usize;
        self.settings_open = false;
        self.selection = None;
        self.selecting = false;
        cx.notify();
    }

    /// Aligns every pane of the active tab to its share of the terminal
    /// area, and records where each one sits for mouse hit-testing. The PTY
    /// receives SIGWINCH only when resizing stops, to avoid redrawing the
    /// shell on every pixel during the drag.
    pub(crate) fn layout_panes(&mut self, window: &Window, cx: &mut Context<Self>) {
        let area = self.terminal_area(window);
        let base = self.content_origin();
        let (cell, line) = (self.cell_width, self.line_height);
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        tab.rect = (
            f32::from(base.x),
            f32::from(base.y),
            f32::from(area.width),
            f32::from(area.height),
        );
        let rects = if tab.zoomed {
            // Only the focused pane is shown; the rest keep their last size.
            vec![(tab.focus_id(), tab.rect)]
        } else {
            tab.tree.layout(tab.rect, f32::from(theme::SPLIT_GAP)).panes
        };
        let mut resizes = Vec::new();
        for (id, (x, y, w, h)) in rects {
            let Some(pane) = tab.panes.iter_mut().find(|p| p.id == id) else {
                continue;
            };
            pane.origin = point(px(x), px(y));
            pane.area = size(px(w), px(h));
            let cols = ((pane.area.width / cell).floor() as usize).max(1);
            let rows = ((pane.area.height / line).floor() as usize).max(1);
            if pane.emu.term.cols == cols && pane.emu.term.rows == rows {
                continue;
            }
            pane.emu.term.resize(cols, rows);
            pane.scroll = pane.scroll.min(pane.emu.term.history_len());
            pane.resize_seq += 1;
            resizes.push((pane.id, pane.resize_seq, (cols as u16, rows as u16)));
        }
        if resizes.is_empty() {
            return;
        }
        self.selection = None;
        for (id, seq, target) in resizes {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(120))
                    .await;
                let _ = this.update(cx, |view, _| view.apply_pty_resize(id, seq, target));
            })
            .detach();
        }
    }

    fn apply_pty_resize(&mut self, id: usize, seq: u64, size: (u16, u16)) {
        if let Some(tab) = self
            .tabs
            .iter_mut()
            .flat_map(|tab| tab.panes.iter_mut())
            .find(|pane| pane.id == id)
        {
            if tab.resize_seq == seq {
                if let Some(pty) = &mut tab.pty {
                    let _ = pty.resize(size.0, size.1);
                    // PowerShell 5.1 non ridisegna il prompt esistente quando
                    // cambia la larghezza: la parte allineata a destra resta
                    // alla vecchia colonna. Un Invio su riga vuota forza un
                    // nuovo prompt alla larghezza corretta. Se l'utente ha
                    // digitato qualcosa, non inviarlo per non eseguire il
                    // comando a sua insaputa.
                    if size.0 != tab.pty_size.0 && cfg!(windows) && !tab.input_pending {
                        let _ = pty.write(b"\r");
                    }
                    tab.pty_size = size;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tab whose shell cannot start: no process, but real panes.
    fn dead_shell() -> Shell {
        Shell {
            name: "none".into(),
            program: "/nonexistent/shell".into(),
        }
    }

    /// Tab with `extra` more panes, each split to the right of the last.
    fn tab(id: usize, extra: usize) -> Tab {
        let shell = dead_shell();
        let mut tab = Tab::terminal(id, &shell, 0, None);
        for k in 0..extra {
            assert!(tab.add_pane(Pane::spawn(100 + k, &shell, None), Axis::Horizontal));
        }
        tab
    }

    #[::core::prelude::v1::test]
    fn a_new_pane_takes_focus_and_a_place_in_the_tree() {
        let mut t = tab(1, 0);
        assert!(t.add_pane(Pane::spawn(2, &dead_shell(), None), Axis::Horizontal));
        t.focus = 0;
        // Split the first pane downwards: a column nested inside the row.
        assert!(t.add_pane(Pane::spawn(3, &dead_shell(), None), Axis::Vertical));
        assert_eq!(t.focus_id(), 3);
        assert_eq!(t.tree.leaves(), vec![1, 3, 2]);
        assert_eq!(t.tree.path_to(3), Some(vec![0, 1]));
        assert!(!t.zoomed);
    }

    #[::core::prelude::v1::test]
    fn removing_moves_focus_to_the_previous_pane_in_tree_order() {
        let mut t = tab(1, 2); // 1 | 100 | 101, focus on 101
        t.remove_pane(2);
        assert_eq!((t.focus_id(), t.tree.leaves()), (100, vec![1, 100]));
        t.focus = 0; // pane 1: nothing before it, the next one inherits
        t.remove_pane(0);
        assert_eq!(
            (t.panes.len(), t.focus_id(), t.tree),
            (1, 100, Tree::Leaf(100))
        );
        // Removing an unfocused pane leaves focus on the same pane.
        let mut t = tab(1, 2);
        t.focus = 1;
        t.remove_pane(0);
        assert_eq!(t.focus_id(), 100);
    }

    #[::core::prelude::v1::test]
    fn exited_panes_close_then_empty_tabs_then_nothing_is_left() {
        let mut tabs = vec![tab(1, 1), tab(2, 0)];
        tabs[0].panes[0].exited = true; // one of two panes
        assert!(prune_exited(&mut tabs));
        assert_eq!((tabs.len(), tabs[0].panes.len()), (2, 1));
        assert_eq!(tabs[0].tree, Tree::Leaf(100));
        tabs[1].panes[0].exited = true; // the only pane of a tab
        assert!(prune_exited(&mut tabs));
        assert_eq!(tabs.len(), 1);
        tabs[0].panes[0].exited = true;
        assert!(prune_exited(&mut tabs) && tabs.is_empty());
        assert!(!prune_exited(&mut tabs));
    }
}
