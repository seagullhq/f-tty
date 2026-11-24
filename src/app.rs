//! Application state: terminal tabs and their lifecycle.

use crate::pty::Pty;
use crate::settings_store;
use crate::shells::{self, Shell};
use crate::term::Emulator;
use crate::{COLS, ROWS, layout};
use gpui::*;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabPosition {
    Top,
    Bottom,
    Left,
    Right,
}

impl TabPosition {
    pub(crate) const ALL: [Self; 4] = [Self::Top, Self::Bottom, Self::Left, Self::Right];

    pub(crate) fn vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }

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

pub(crate) struct Tab {
    pub(crate) id: usize,
    pub(crate) pty: Option<Pty>,
    pub(crate) emu: Emulator,
    pub(crate) title: String,
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
    /// Last size applied to the PTY, used to detect column changes.
    pub(crate) pty_size: (u16, u16),
}

impl Tab {
    fn terminal(id: usize, shell: &Shell) -> Self {
        let mut emu = Emulator::new(COLS, ROWS);
        let pty = match Pty::spawn(COLS as u16, ROWS as u16, &shell.program) {
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
            title: format!("Terminal {id}"),
            resize_seq: 0,
            scroll: 0,
            scroll_acc: 0.,
            input_pending: false,
            pty_size: (COLS as u16, ROWS as u16),
        }
    }
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
    pub(crate) shells: Vec<Shell>,
    pub(crate) shell: Shell,
    pub(crate) settings_open: bool,
    pub(crate) cell_width: Pixels,
    pub(crate) line_height: Pixels,
    pub(crate) selection: Option<Selection>,
    pub(crate) selecting: bool,
    /// Family used by the terminal grid; changeable from settings.
    pub(crate) font: SharedString,
    pub(crate) fonts: Vec<String>,
    pub(crate) font_query: String,
    /// Whether the font combobox dropdown is expanded.
    pub(crate) font_picker_open: bool,
}

impl TerminalView {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus);
        window.set_window_title("ftty");

        let saved = settings_store::load();
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
            tabs: vec![Tab::terminal(1, &shell)],
            active: 0,
            next_id: 2,
            tabs_position: saved.tabs_position,
            shells,
            shell,
            settings_open: false,
            cell_width,
            line_height,
            selection: None,
            selecting: false,
            font: font_family.into(),
            fonts,
            font_query: String::new(),
            font_picker_open: false,
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
    fn save_settings(&self) {
        settings_store::save(&settings_store::SavedSettings {
            tabs_position: self.tabs_position,
            shell: self.shell.program.clone(),
            font: self.font.to_string(),
        });
    }

    /// Applies a font family to new renders and re-aligns the grid to it.
    pub(crate) fn select_font(
        &mut self,
        family: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.font_picker_open = false;
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

    /// Changes the tab list side while keeping the terminal the same size:
    /// the window grows/shrinks by the space taken by the bar.
    pub(crate) fn set_tab_position(
        &mut self,
        position: TabPosition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection = None;
        self.selecting = false;
        if position != self.tabs_position {
            let (dw, dh) = layout::size_delta(self.tabs_position, position);
            self.tabs_position = position;
            if !window.is_maximized() && !window.is_fullscreen() {
                let current = window.bounds().size;
                let target = size(current.width + dw, current.height + dh);
                let min = layout::min_size(position);
                window.resize(size(
                    if target.width < min.width {
                        min.width
                    } else {
                        target.width
                    },
                    if target.height < min.height {
                        min.height
                    } else {
                        target.height
                    },
                ));
            }
            self.save_settings();
        }
        cx.notify();
    }

    /// Drains chunks from all tabs and notifies on output or title changes.
    fn pump(&mut self, cx: &mut Context<Self>) {
        let mut active_received = false;
        let mut title_changed = false;
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            let mut received = false;
            let history_before = tab.emu.term.history_len();
            let mut dsr: Option<Vec<u8>> = None;
            if let Some(pty) = &tab.pty {
                while let Ok(chunk) = pty.rx.try_recv() {
                    tab.emu.feed(&chunk);
                    if tab.emu.term.dsr_pending {
                        tab.emu.term.dsr_pending = false;
                        let (row, col) = tab.emu.term.cursor;
                        dsr = Some(format!("\x1b[{};{}R", row + 1, col + 1).into_bytes());
                    }
                    received = true;
                }
            }
            // Rispondi al DSR fuori dal borrow dell'emulatore: altrimenti
            // ConPTY resta in attesa e la shell su Windows non parte.
            if let Some(response) = dsr {
                if let Some(pty) = &mut tab.pty {
                    let _ = pty.write(&response);
                }
            }
            let added = tab.emu.term.history_len() - history_before;
            if tab.scroll > 0 && added > 0 {
                // The text in view stays put while output arrives.
                tab.scroll = (tab.scroll + added).min(tab.emu.term.history_len());
            }
            if received {
                let new_title = tab.emu.term.display_title(tab.id);
                if new_title != tab.title {
                    tab.title = new_title;
                    title_changed = true;
                }
                if i == self.active {
                    active_received = true;
                }
            }
        }
        if active_received || title_changed {
            cx.notify();
        }
    }

    pub(crate) fn open_tab(&mut self, cx: &mut Context<Self>) {
        self.tabs.push(Tab::terminal(self.next_id, &self.shell));
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

    /// Aligns the grid to the available space; the PTY receives SIGWINCH
    /// only when resizing stops, to avoid redrawing the shell on every
    /// pixel during the drag.
    pub(crate) fn resize_active_terminal(
        &mut self,
        cols: usize,
        rows: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if tab.emu.term.cols == cols && tab.emu.term.rows == rows {
            return;
        }
        tab.emu.term.resize(cols, rows);
        tab.scroll = tab.scroll.min(tab.emu.term.history_len());
        self.selection = None;
        tab.resize_seq += 1;
        let (id, seq) = (tab.id, tab.resize_seq);
        let target = (cols as u16, rows as u16);
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(120))
                .await;
            let _ = this.update(cx, |view, _| view.apply_pty_resize(id, seq, target));
        })
        .detach();
    }

    fn apply_pty_resize(&mut self, id: usize, seq: u64, size: (u16, u16)) {
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
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
