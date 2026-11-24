use gpui::*;
use std::time::Duration;

mod assets;
mod pty;
mod shells;
mod term;
mod theme;

use pty::Pty;
use shells::Shell;
use term::{Cell, DEFAULT_BG, DEFAULT_FG, Emulator, Style, Term};

const FONT: &str = if cfg!(windows) {
    "Consolas"
} else if cfg!(target_os = "macos") {
    "Menlo"
} else {
    "Noto Sans Mono"
};
const COLS: usize = 80;
const ROWS: usize = 24;
const WINDOW_SIZE: Size<Pixels> = size(px(696.), px(608.));
/// Window floor: below the size that shows 80×24 the terminal is clipped,
/// but the window stays resizable.
const MIN_WINDOW_SIZE: Size<Pixels> = size(px(320.), px(200.));

fn color_of(c: u32) -> Hsla {
    rgb(c).into()
}

/// Monochrome Heroicons icon, tinted with the theme color.
fn icon(path: &'static str, size: f32, color: u32) -> Svg {
    svg()
        .path(path)
        .w(px(size))
        .h(px(size))
        .text_color(color_of(color))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TabPosition {
    Top,
    Bottom,
    Left,
    Right,
}

impl TabPosition {
    const ALL: [Self; 4] = [Self::Top, Self::Bottom, Self::Left, Self::Right];

    fn vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Bottom => "Bottom",
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }
}

struct Tab {
    id: usize,
    pty: Option<Pty>,
    emu: Emulator,
    title: String,
    /// Changes on every grid resize: PTY resizes only start when the drag
    /// stops (the latest generation wins).
    resize_seq: u64,
    /// Scrollback lines shown above the screen (0 = bottom).
    scroll: usize,
    /// Accumulator for fractional touchpad deltas.
    scroll_acc: f32,
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
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Selection {
    anchor: (usize, usize),
    cursor: (usize, usize),
}

struct TerminalView {
    focus: FocusHandle,
    tabs: Vec<Tab>,
    active: usize,
    next_id: usize,
    tabs_position: TabPosition,
    shells: Vec<Shell>,
    shell: Shell,
    settings_open: bool,
    cell_width: Pixels,
    line_height: Pixels,
    selection: Option<Selection>,
    selecting: bool,
}

impl TerminalView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus);
        window.set_window_title("ftty");

        let shells = shells::available();
        let shell = shells::default_shell(&shells).expect("no shell available");

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

        let font_size = window.rem_size() * 0.875;
        let run = TextRun {
            len: 1,
            font: font(FONT),
            color: black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let cell_width = window
            .text_system()
            .shape_line("M".into(), font_size, &[run], None)
            .width;
        let cell_width = if cell_width > px(0.) {
            cell_width
        } else {
            px(1.)
        };
        // TextStyle::default() uses phi() = 1.618034.
        let line_height = font_size * 1.618034;

        Self {
            focus,
            tabs: vec![Tab::terminal(1, &shell)],
            active: 0,
            next_id: 2,
            tabs_position: TabPosition::Top,
            shells,
            shell,
            settings_open: false,
            cell_width,
            line_height,
            selection: None,
            selecting: false,
        }
    }

    /// Changes the tab list side while keeping the terminal the same size:
    /// the window grows/shrinks by the space taken by the bar.
    fn set_tab_position(
        &mut self,
        position: TabPosition,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection = None;
        self.selecting = false;
        if position != self.tabs_position {
            let (dw, dh) = layout_size_delta(self.tabs_position, position);
            self.tabs_position = position;
            if !window.is_maximized() && !window.is_fullscreen() {
                let current = window.bounds().size;
                let target = size(current.width + dw, current.height + dh);
                let min = layout_min_size(position);
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
        }
        cx.notify();
    }

    /// Drains chunks from all tabs and notifies only if the active one changed.
    fn pump(&mut self, cx: &mut Context<Self>) {
        let mut active_received = false;
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
            if received && i == self.active {
                active_received = true;
            }
        }
        if active_received {
            cx.notify();
        }
    }

    fn open_tab(&mut self, cx: &mut Context<Self>) {
        self.tabs.push(Tab::terminal(self.next_id, &self.shell));
        self.next_id += 1;
        self.active = self.tabs.len() - 1;
        self.settings_open = false;
        self.selection = None;
        self.selecting = false;
        cx.notify();
    }

    /// Shows/hides settings in the terminal area.
    fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_open = !self.settings_open;
        self.selection = None;
        self.selecting = false;
        cx.notify();
    }

    /// Sets the shell used by new tabs.
    fn select_shell(&mut self, shell: Shell, cx: &mut Context<Self>) {
        self.shell = shell;
        cx.notify();
    }

    fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
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
        self.active = active_after_close(self.active, index, self.tabs.len());
        cx.notify();
    }

    fn cycle_tab(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.tabs.len() < 2 {
            return;
        }
        self.active = (self.active as isize + delta).rem_euclid(self.tabs.len() as isize) as usize;
        self.settings_open = false;
        self.selection = None;
        self.selecting = false;
        cx.notify();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;

        // Tab shortcuts (not sent to the shell).
        if ks.modifiers.control {
            if ks.modifiers.shift {
                match ks.key.as_str() {
                    "c" => {
                        self.copy_selection(cx);
                        return;
                    }
                    "t" => return self.open_tab(cx),
                    "w" => {
                        if self.settings_open {
                            self.settings_open = false;
                            cx.notify();
                        } else {
                            self.close_tab(self.active, window, cx);
                        }
                        return;
                    }
                    "tab" => return self.cycle_tab(-1, cx),
                    _ => {}
                }
            } else if ks.key == "tab" {
                return self.cycle_tab(1, cx);
            }
        }

        if self.settings_open {
            return;
        }

        if ks.modifiers.shift && matches!(ks.key.as_str(), "pageup" | "pagedown") {
            self.scroll_page(ks.key == "pageup", cx);
            return;
        }

        let bytes: Option<Vec<u8>> = match ks.key.as_str() {
            "enter" => Some(b"\r".to_vec()),
            "backspace" => Some(vec![0x7f]),
            "tab" => Some(b"\t".to_vec()),
            "escape" => Some(vec![0x1b]),
            "up" => Some(b"\x1b[A".to_vec()),
            "down" => Some(b"\x1b[B".to_vec()),
            "right" => Some(b"\x1b[C".to_vec()),
            "left" => Some(b"\x1b[D".to_vec()),
            "home" => Some(b"\x1b[H".to_vec()),
            "end" => Some(b"\x1b[F".to_vec()),
            "delete" => Some(b"\x1b[3~".to_vec()),
            key if ks.modifiers.control && key.len() == 1 => Some(vec![key.as_bytes()[0] & 0x1f]),
            _ => ks.key_char.clone().map(|s| s.into_bytes()),
        };

        if let Some(b) = bytes {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                // Typing always jumps back to the present.
                tab.scroll = 0;
                tab.scroll_acc = 0.;
                if let Some(pty) = &mut tab.pty {
                    let _ = pty.write(&b);
                }
            }
        }
    }

    fn text_origin(&self) -> Point<Pixels> {
        let mut x = theme::GAP + px(1.) + theme::CONTENT_PADDING;
        if self.tabs_position == TabPosition::Left {
            x += theme::STRIP_WIDTH + theme::GAP;
        }
        point(x, theme::BAR_HEIGHT + px(1.) + px(2.))
    }

    fn terminal_area(&self, window: &Window) -> Size<Pixels> {
        let bounds = window.bounds().size;
        let (strip_w, strip_h) = match self.tabs_position {
            TabPosition::Top => (px(0.), px(0.)),
            TabPosition::Bottom => (px(0.), theme::TAB_HEIGHT + theme::GAP),
            TabPosition::Left | TabPosition::Right => (theme::STRIP_WIDTH + theme::GAP, px(0.)),
        };
        let width = bounds.width - theme::GAP * 2. - strip_w - px(2.) - theme::CONTENT_PADDING * 2.;
        let height = bounds.height
            - theme::BAR_HEIGHT
            - theme::GAP
            - strip_h
            - px(2.)
            - px(2.)
            - theme::CONTENT_PADDING;
        size(
            if width > px(1.) { width } else { px(1.) },
            if height > px(1.) { height } else { px(1.) },
        )
    }

    /// Aligns the grid to the available space; the PTY receives SIGWINCH
    /// only when resizing stops, to avoid redrawing the shell on every
    /// pixel during the drag.
    fn resize_active_terminal(&mut self, cols: usize, rows: usize, cx: &mut Context<Self>) {
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
                if let Some(pty) = &tab.pty {
                    let _ = pty.resize(size.0, size.1);
                }
            }
        }
    }

    /// Positive lines = go back in history, negative = return to the present.
    fn scroll_active(&mut self, lines: f32, cx: &mut Context<Self>) {
        if self.settings_open {
            return;
        }
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let target = apply_scroll(
            tab.scroll,
            tab.emu.term.history_len(),
            lines,
            &mut tab.scroll_acc,
        );
        if target != tab.scroll {
            tab.scroll = target;
            self.selection = None;
            self.selecting = false;
            cx.notify();
        }
    }

    fn scroll_page(&mut self, up: bool, cx: &mut Context<Self>) {
        let page = self
            .tabs
            .get(self.active)
            .map(|tab| tab.emu.term.rows.saturating_sub(1).max(1))
            .unwrap_or(1) as f32;
        self.scroll_active(if up { page } else { -page }, cx);
    }

    fn on_terminal_scroll(
        &mut self,
        ev: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let lines = match ev.delta {
            ScrollDelta::Lines(delta) => delta.y,
            ScrollDelta::Pixels(delta) => delta.y / self.line_height,
        };
        self.scroll_active(lines, cx);
    }

    fn cell_at(&self, position: Point<Pixels>) -> (usize, usize) {
        let term = &self.tabs[self.active].emu.term;
        let origin = self.text_origin();
        let col = ((position.x - origin.x) / self.cell_width).floor();
        let row = ((position.y - origin.y) / self.line_height).floor();
        (
            (row.max(0.) as usize).min(term.rows.saturating_sub(1)),
            (col.max(0.) as usize).min(term.cols.saturating_sub(1)),
        )
    }

    fn on_terminal_mouse_down(
        &mut self,
        ev: &MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_open || self.tabs[self.active].scroll > 0 {
            return;
        }
        let cell = self.cell_at(ev.position);
        self.selection = Some(Selection {
            anchor: cell,
            cursor: cell,
        });
        self.selecting = true;
        cx.notify();
    }

    fn on_terminal_mouse_move(
        &mut self,
        ev: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.selecting || ev.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let cell = self.cell_at(ev.position);
        if let Some(selection) = &mut self.selection {
            if selection.cursor != cell {
                selection.cursor = cell;
                cx.notify();
            }
        }
    }

    fn on_terminal_mouse_up(
        &mut self,
        _ev: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.selecting {
            return;
        }
        self.selecting = false;
        if self
            .selection
            .is_some_and(|selection| selection.anchor == selection.cursor)
        {
            self.selection = None;
        }
        cx.notify();
    }

    /// Linear range [start, end) of the selection.
    fn selection_range(&self) -> Option<(usize, usize)> {
        let selection = self.selection?;
        let cols = self.tabs[self.active].emu.term.cols;
        let a = selection.anchor.0 * cols + selection.anchor.1;
        let b = selection.cursor.0 * cols + selection.cursor.1;
        Some((a.min(b), a.max(b) + 1))
    }

    /// Highlights the selection with translucent rectangles above the text:
    /// text layout does not change, so dragging stays smooth.
    fn selection_overlays(&self, term: &Term) -> Vec<Div> {
        let Some((start, end)) = self.selection_range() else {
            return Vec::new();
        };
        if term.cols == 0 || start >= end {
            return Vec::new();
        }
        let color = color_of(theme::SELECTION).opacity(0.4);
        let mut rects = Vec::new();
        let first_row = start / term.cols;
        let last_row = (end - 1) / term.cols;
        for row in first_row..=last_row {
            let row_start = start.max(row * term.cols);
            let row_end = end.min((row + 1) * term.cols);
            if row_start >= row_end {
                continue;
            }
            let col = row_start - row * term.cols;
            let cells = row_end - row_start;
            rects.push(
                div()
                    .absolute()
                    .left(self.cell_width * col as f32)
                    .top(self.line_height * row as f32)
                    .w(self.cell_width * cells as f32)
                    .h(self.line_height)
                    .bg(color),
            );
        }
        rects
    }

    fn copy_selection(&self, cx: &mut Context<Self>) {
        let Some(selection) = self.selection else {
            return;
        };
        let term = &self.tabs[self.active].emu.term;
        let text = selection_text(term, selection.anchor, selection.cursor);
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }
}

/// Applies a delta in lines (positive = back), keeping touchpad fractional
/// remainders; the result is always within the history.
fn apply_scroll(scroll: usize, history: usize, delta: f32, acc: &mut f32) -> usize {
    let total = *acc + delta;
    let whole = total.trunc();
    *acc = total - whole;
    (scroll as f32 + whole).clamp(0., history as f32) as usize
}

/// Selected text between two cells (inclusive), rows joined by newline and
/// trailing spaces trimmed like terminals do.
fn selection_text(term: &Term, anchor: (usize, usize), cursor: (usize, usize)) -> String {
    if term.cols == 0 || term.rows == 0 {
        return String::new();
    }
    let (start, end) = if anchor <= cursor {
        (anchor, cursor)
    } else {
        (cursor, anchor)
    };
    let start_index = start.0 * term.cols + start.1;
    let end_index = (end.0 * term.cols + end.1).min(term.cells.len().saturating_sub(1));
    if start_index > end_index {
        return String::new();
    }
    let mut text = String::new();
    let mut row = start.0;
    let mut line = String::new();
    for index in start_index..=end_index {
        let r = index / term.cols;
        if r != row {
            text.push_str(line.trim_end());
            text.push('\n');
            line.clear();
            row = r;
        }
        line.push(term.cells[index].ch);
    }
    text.push_str(line.trim_end());
    text
}

/// Active index after closing `removed`, with at least one tab left.
fn active_after_close(active: usize, removed: usize, remaining: usize) -> usize {
    let active = if removed < active { active - 1 } else { active };
    active.min(remaining - 1)
}

/// Minimum window size for the current layout: side tabs need more width,
/// bottom tabs need more height.
fn layout_min_size(position: TabPosition) -> Size<Pixels> {
    match position {
        TabPosition::Top => MIN_WINDOW_SIZE,
        TabPosition::Bottom => size(
            MIN_WINDOW_SIZE.width,
            MIN_WINDOW_SIZE.height + theme::TAB_HEIGHT + theme::GAP,
        ),
        TabPosition::Left | TabPosition::Right => size(
            MIN_WINDOW_SIZE.width + theme::STRIP_WIDTH + theme::GAP,
            MIN_WINDOW_SIZE.height,
        ),
    }
}

/// Window size delta when the tab list changes side, so the terminal keeps
/// the same columns/rows.
fn layout_size_delta(old: TabPosition, new: TabPosition) -> (Pixels, Pixels) {
    let width = match (old.vertical(), new.vertical()) {
        (true, false) => -(theme::STRIP_WIDTH + theme::GAP),
        (false, true) => theme::STRIP_WIDTH + theme::GAP,
        _ => px(0.),
    };
    let height = match (old == TabPosition::Bottom, new == TabPosition::Bottom) {
        (true, false) => -(theme::TAB_HEIGHT + theme::GAP),
        (false, true) => theme::TAB_HEIGHT + theme::GAP,
        _ => px(0.),
    };
    (width, height)
}

/// Invisible edge zone: captures the drag and delegates resizing to the compositor.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn resize_handle(zone: Div, edge: ResizeEdge) -> Div {
    zone.on_mouse_down(MouseButton::Left, move |_, window, cx| {
        cx.stop_propagation();
        window.start_window_resize(edge);
    })
}

/// On Windows `zoom_window()` only maximizes: the "restore" button needs
/// SW_RESTORE, which GPUI does not expose.
#[cfg(windows)]
fn restore_maximized(window: &Window) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    #[link(name = "user32")]
    unsafe extern "system" {
        fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    }

    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return;
    };
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        const SW_RESTORE: i32 = 9;
        unsafe {
            ShowWindow(handle.hwnd.get(), SW_RESTORE);
        }
    }
}

fn maximize_or_restore(window: &Window) {
    #[cfg(windows)]
    {
        if window.is_maximized() {
            restore_maximized(window);
        } else {
            window.zoom_window();
        }
    }
    #[cfg(not(windows))]
    window.zoom_window();
}

/// Zed-style window control: clickable area with hover, no system chrome.
fn window_control(
    id: &'static str,
    icon: impl IntoElement,
    hover_bg: u32,
    action: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let button = div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .flex_none()
        .cursor_pointer()
        .hover(move |s| s.bg(color_of(hover_bg)))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(move |_, window, cx| {
            cx.stop_propagation();
            action(window, cx);
        })
        .child(icon);

    // Linux: round buttons. Windows: full-height caption without gaps.
    #[cfg(windows)]
    {
        button.w(px(40.)).h_full()
    }
    #[cfg(not(windows))]
    {
        button
            .w(theme::CONTROL_SIZE)
            .h(theme::CONTROL_SIZE)
            .rounded_full()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct RunKey {
    fg: u32,
    bg: u32,
    bold: bool,
}

/// Length to draw: trailing unstyled spaces should not be laid out or
/// painted, but the cursor must stay visible even on an empty cell.
fn line_len(cells: &[Cell], cursor_col: Option<usize>) -> usize {
    let mut len = cells.len();
    while len > 0 {
        let cell = &cells[len - 1];
        if cell.ch == ' ' && cell.style == Style::default() && cursor_col != Some(len - 1) {
            len -= 1;
        } else {
            break;
        }
    }
    len
}

/// Builds a row as a single text with contiguous style runs.
fn line_from_cells(cells: &[Cell], cursor_col: Option<usize>) -> Div {
    let len = line_len(cells, cursor_col);

    let regular = font(FONT);
    let bold = Font {
        weight: FontWeight::BOLD,
        ..font(FONT)
    };

    let mut text = String::with_capacity(len);
    let mut runs: Vec<TextRun> = Vec::new();
    let mut last: Option<RunKey> = None;

    for (col, cell) in cells[..len].iter().enumerate() {
        let is_cursor = cursor_col == Some(col);
        let mut fg = cell.style.fg.resolve(DEFAULT_FG);
        let mut bg = cell.style.bg.resolve(DEFAULT_BG);
        if cell.style.inverse ^ is_cursor {
            std::mem::swap(&mut fg, &mut bg);
        }

        let key = RunKey {
            fg,
            bg,
            bold: cell.style.bold,
        };

        if last == Some(key) {
            if let Some(run) = runs.last_mut() {
                run.len += cell.ch.len_utf8();
            }
        } else {
            runs.push(TextRun {
                len: cell.ch.len_utf8(),
                font: if key.bold {
                    bold.clone()
                } else {
                    regular.clone()
                },
                color: color_of(fg),
                background_color: (bg != DEFAULT_BG).then(|| color_of(bg)),
                underline: None,
                strikethrough: None,
            });
            last = Some(key);
        }
        text.push(cell.ch);
    }

    div().child(StyledText::new(text).with_runs(runs))
}

impl TerminalView {
    /// Top bar: brand, tabs (if at top), drag area, layout selector, window
    /// controls. Active tabs share the terminal background.
    fn chrome(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let mut bar = div()
            .h(theme::BAR_HEIGHT)
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .pl(theme::BAR_PADDING)
            .pr(if cfg!(windows) {
                px(0.)
            } else {
                theme::BAR_PADDING
            });

        // On Windows dragging goes through WM_NCHITTEST (WindowControlArea),
        // not through client mouse events.
        #[cfg(not(windows))]
        {
            bar = bar.on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move());
        }

        // macOS: space for the native traffic lights at the top left.
        if cfg!(target_os = "macos") {
            bar = bar.child(div().flex_none().w(px(66.)));
        }

        bar = bar.child(
            div()
                .flex_none()
                .w(px(2.))
                .h(px(12.))
                .rounded(px(1.))
                .bg(color_of(theme::ACCENT)),
        );

        if self.tabs_position == TabPosition::Top {
            for i in 0..self.tabs.len() {
                bar = bar.child(self.tab(i, false, cx));
            }
            bar = bar.child(self.new_tab_button(cx));
        } else {
            bar = bar.child(
                div()
                    .h_full()
                    .flex()
                    .items_center()
                    .text_xs()
                    .text_color(color_of(theme::MUTED))
                    .window_control_area(WindowControlArea::Drag)
                    .child("ftty"),
            );
        }

        let right = if cfg!(target_os = "macos") {
            // Window controls handled by the system (traffic lights).
            div().flex().items_center().child(self.settings_button(cx))
        } else {
            div()
                .flex()
                .items_center()
                .h_full()
                .gap(theme::CONTROL_GAP)
                .child(self.settings_button(cx))
                .child(self.window_controls(window))
        };

        bar.child(
            div()
                .flex_1()
                .h_full()
                .window_control_area(WindowControlArea::Drag),
        )
        .child(right)
    }

    /// Caption buttons: round on Linux, full height on Windows.
    fn window_controls(&self, window: &Window) -> impl IntoElement {
        let icon_size = if cfg!(windows) { 12. } else { 11. };
        let (maximize_id, maximize_icon) = if window.is_maximized() {
            ("restore", "icons/square-2-stack.svg")
        } else {
            ("maximize", "icons/square.svg")
        };
        div()
            .flex()
            .items_center()
            .h_full()
            .gap(if cfg!(windows) {
                px(0.)
            } else {
                theme::CONTROL_GAP
            })
            .child(window_control(
                "minimize",
                icon("icons/minus.svg", icon_size, theme::MUTED),
                theme::HOVER,
                |window, _| window.minimize_window(),
            ))
            .child(window_control(
                maximize_id,
                icon(maximize_icon, icon_size, theme::MUTED),
                theme::HOVER,
                |window, _| maximize_or_restore(window),
            ))
            .child(window_control(
                "close",
                icon("icons/x-mark.svg", icon_size, DEFAULT_FG),
                theme::DANGER,
                |window, _| window.remove_window(),
            ))
    }

    /// A horizontal tab (bar) or vertical tab (side panel).
    fn tab(&self, i: usize, vertical: bool, cx: &mut Context<Self>) -> impl IntoElement {
        // With Settings open no tab is active: the space belongs to settings.
        let active = !self.settings_open && i == self.active;
        let group = format!("tab-{i}");

        let close = div()
            .id(("tab-close", i))
            .flex()
            .items_center()
            .justify_center()
            .w(px(16.))
            .h(px(16.))
            .mr(px(4.))
            .rounded(theme::CHIP_RADIUS)
            .cursor_pointer()
            .text_color(color_of(theme::MUTED))
            .hover(|s| {
                s.bg(color_of(theme::HOVER))
                    .text_color(color_of(DEFAULT_FG))
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, window, cx| {
                this.close_tab(i, window, cx);
            }))
            .child(icon("icons/x-mark.svg", 12., theme::MUTED));

        let close = if active {
            close
        } else {
            // On inactive tabs the × only appears on hover.
            close
                .group_hover(group.clone(), |s| s.opacity(1.))
                .opacity(0.)
        };

        let tab = div()
            .id(("tab", i))
            .group(group)
            .flex()
            .items_center()
            .h(theme::TAB_HEIGHT)
            .rounded(theme::TAB_RADIUS)
            .overflow_hidden()
            .bg(if active {
                color_of(theme::ACTIVE)
            } else {
                color_of(theme::CHROME)
            })
            .hover(move |s| {
                if active {
                    s
                } else {
                    s.bg(color_of(theme::HOVER))
                }
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());

        let tab = if vertical {
            tab.w_full()
        } else {
            tab.w(theme::TAB_WIDTH).min_w(theme::TAB_MIN_WIDTH)
        };

        tab.child(
            div()
                .id(("tab-title", i))
                .flex()
                .flex_1()
                .items_center()
                .h_full()
                .pl(theme::TAB_PADDING)
                .truncate()
                .cursor_pointer()
                .text_xs()
                .text_color(color_of(if active { DEFAULT_FG } else { theme::MUTED }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.active = i;
                    this.settings_open = false;
                    this.selection = None;
                    this.selecting = false;
                    cx.notify();
                }))
                .child(self.tabs[i].title.clone()),
        )
        .child(close)
    }

    fn new_tab_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("new-tab")
            .flex()
            .items_center()
            .justify_center()
            .flex_none()
            .w(px(26.))
            .h(px(26.))
            .rounded(theme::TAB_RADIUS)
            .cursor_pointer()
            .text_size(px(16.))
            .text_color(color_of(theme::MUTED))
            .hover(|s| {
                s.bg(color_of(theme::HOVER))
                    .text_color(color_of(DEFAULT_FG))
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| this.open_tab(cx)))
            .child(icon("icons/plus.svg", 12., theme::MUTED))
    }

    /// Opens/closes the settings view.
    fn settings_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.settings_open;
        div()
            .id("settings")
            .flex()
            .items_center()
            .justify_center()
            .flex_none()
            .w(theme::CONTROL_SIZE)
            .h(theme::CONTROL_SIZE)
            .rounded(theme::CONTROL_RADIUS)
            .bg(color_of(if active { theme::ACTIVE } else { theme::CHROME }))
            .cursor_pointer()
            .hover(|s| s.bg(color_of(theme::HOVER)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| this.toggle_settings(cx)))
            .child(icon(
                "icons/cog-6-tooth.svg",
                14.,
                if active { DEFAULT_FG } else { theme::MUTED },
            ))
    }

    /// Settings content: tab position and default shell.
    fn settings(&self, cx: &mut Context<Self>) -> Div {
        let mut view = div()
            .flex_1()
            .w_full()
            .min_w(px(0.))
            .min_h(px(0.))
            .overflow_hidden()
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(20.))
            .text_xs();

        let mut positions = div().flex().flex_wrap().gap_1();
        for (i, position) in TabPosition::ALL.into_iter().enumerate() {
            let current = position == self.tabs_position;
            positions = positions.child(
                div()
                    .id(("settings-layout", i))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(30.))
                    .px(px(10.))
                    .rounded(theme::CONTROL_RADIUS)
                    .bg(color_of(if current {
                        theme::ACTIVE
                    } else {
                        theme::CHROME
                    }))
                    .text_color(color_of(if current { DEFAULT_FG } else { theme::MUTED }))
                    .cursor_pointer()
                    .hover(|s| s.bg(color_of(theme::HOVER)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.set_tab_position(position, window, cx);
                    }))
                    .child(layout_icon(position))
                    .child(position.label()),
            );
        }
        view = view.child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(section_label("Tab position"))
                .child(positions),
        );

        let mut shells = div().flex().flex_col().gap_1();
        for (i, shell) in self.shells.iter().enumerate() {
            let current = shell.program == self.shell.program;
            let name = shell.name.clone();
            let program = shell.program.clone();
            let shell = shell.clone();
            shells = shells.child(
                div()
                    .id(("settings-shell", i))
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(30.))
                    .px(px(8.))
                    .rounded(theme::CONTROL_RADIUS)
                    .text_color(color_of(if current { DEFAULT_FG } else { theme::MUTED }))
                    .cursor_pointer()
                    .hover(|s| s.bg(color_of(theme::HOVER)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_shell(shell.clone(), cx);
                    }))
                    .child(
                        div()
                            .flex_none()
                            .w(px(2.))
                            .h(px(12.))
                            .bg(color_of(if current {
                                theme::ACCENT
                            } else {
                                theme::CHROME
                            })),
                    )
                    .child(name)
                    .child(div().flex_1())
                    .child(
                        div()
                            .truncate()
                            .text_color(color_of(theme::MUTED))
                            .child(program),
                    ),
            );
        }
        view = view.child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(section_label("Default shell"))
                .child(shells)
                .child(
                    div()
                        .text_color(color_of(theme::MUTED))
                        .child("New tabs use the selected shell."),
                ),
        );

        view
    }

    /// Invisible edges to resize the window without system decorations (CSD):
    /// 6px on the sides, 10px on the corners.
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn resize_zones(&self) -> Div {
        let edge = px(6.);
        let corner = px(10.);

        let mut zones = div().absolute().top(px(0.)).left(px(0.)).size_full();

        // On Windows caption buttons take the whole top border: no resize
        // zones there.
        if !cfg!(windows) {
            zones = zones
                .child(resize_handle(
                    div()
                        .absolute()
                        .top(px(0.))
                        .left(px(0.))
                        .w_full()
                        .h(edge)
                        .cursor_ns_resize(),
                    ResizeEdge::Top,
                ))
                .child(resize_handle(
                    div()
                        .absolute()
                        .top(px(0.))
                        .left(px(0.))
                        .size(corner)
                        .cursor_nwse_resize(),
                    ResizeEdge::TopLeft,
                ))
                .child(resize_handle(
                    div()
                        .absolute()
                        .top(px(0.))
                        .right(px(0.))
                        .size(corner)
                        .cursor_nesw_resize(),
                    ResizeEdge::TopRight,
                ));
        }

        zones
            .child(resize_handle(
                div()
                    .absolute()
                    .bottom(px(0.))
                    .left(px(0.))
                    .w_full()
                    .h(edge)
                    .cursor_ns_resize(),
                ResizeEdge::Bottom,
            ))
            .child(resize_handle(
                div()
                    .absolute()
                    .top(px(0.))
                    .bottom(px(0.))
                    .left(px(0.))
                    .w(edge)
                    .cursor_ew_resize(),
                ResizeEdge::Left,
            ))
            .child(resize_handle(
                div()
                    .absolute()
                    .top(if cfg!(windows) {
                        theme::BAR_HEIGHT
                    } else {
                        px(0.)
                    })
                    .bottom(px(0.))
                    .right(px(0.))
                    .w(edge)
                    .cursor_ew_resize(),
                ResizeEdge::Right,
            ))
            .child(resize_handle(
                div()
                    .absolute()
                    .bottom(px(0.))
                    .left(px(0.))
                    .size(corner)
                    .cursor_nesw_resize(),
                ResizeEdge::BottomLeft,
            ))
            .child(resize_handle(
                div()
                    .absolute()
                    .bottom(px(0.))
                    .right(px(0.))
                    .size(corner)
                    .cursor_nwse_resize(),
                ResizeEdge::BottomRight,
            ))
    }

    /// Bottom tab list: rounded chips above the window edge.
    fn bottom_tabs(&self, cx: &mut Context<Self>) -> Div {
        let mut row = div().w_full().flex_none().flex().items_center().gap_1();

        for i in 0..self.tabs.len() {
            row = row.child(self.tab(i, false, cx));
        }

        row.child(self.new_tab_button(cx)).child(div().flex_1())
    }

    /// Vertical side tab list, browser style, as chips on a single surface.
    fn side_tabs(&self, cx: &mut Context<Self>) -> Div {
        let mut panel = div()
            .flex_none()
            .w(theme::STRIP_WIDTH)
            .h_full()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .h(theme::TAB_HEIGHT)
                    .w_full()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .pl(theme::TAB_PADDING)
                    .pr(px(2.))
                    .text_xs()
                    .text_color(color_of(theme::MUTED))
                    .child("Tabs")
                    .child(self.new_tab_button(cx)),
            );

        for i in 0..self.tabs.len() {
            panel = panel.child(self.tab(i, true, cx));
        }

        panel
    }
}

fn content_card(content: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .w_full()
        .min_w(px(0.))
        .min_h(px(0.))
        .px(theme::CONTENT_PADDING)
        .pt(px(2.))
        .pb(theme::CONTENT_PADDING)
        .bg(color_of(DEFAULT_BG))
        .border_1()
        .border_color(color_of(theme::BORDER))
        .rounded(theme::SURFACE_RADIUS)
        .overflow_hidden()
        .child(content)
}

fn section_label(text: &'static str) -> Div {
    div()
        .text_xs()
        .text_color(color_of(theme::MUTED))
        .child(text)
}

/// Selector icon: rectangle with the band on the tab list side.
fn layout_icon(position: TabPosition) -> Div {
    let icon = div()
        .w(px(12.))
        .h(px(10.))
        .border_1()
        .border_color(color_of(theme::MUTED));
    let band = || div().bg(color_of(theme::MUTED));

    match position {
        TabPosition::Top => icon.flex().flex_col().child(band().w_full().h(px(3.))),
        TabPosition::Bottom => icon
            .flex()
            .flex_col()
            .justify_end()
            .child(band().w_full().h(px(3.))),
        TabPosition::Left => icon.flex().child(band().w(px(3.)).h_full()),
        TabPosition::Right => icon.flex().justify_end().child(band().w(px(3.)).h_full()),
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let area = self.terminal_area(window);
        let cols = ((area.width / self.cell_width).floor() as usize).max(1);
        let rows = ((area.height / self.line_height).floor() as usize).max(1);
        self.resize_active_terminal(cols, rows, cx);

        let card = if self.settings_open {
            content_card(self.settings(cx))
        } else {
            let tab = &self.tabs[self.active];
            let term = &tab.emu.term;
            let top = term.visible_top(tab.scroll);
            let cursor = (tab.scroll == 0 && term.cursor_visible)
                .then(|| (term.history_len() + term.cursor.0, term.cursor.1));
            let lines: Vec<Div> = (0..term.rows)
                .map(|r| {
                    let index = top + r;
                    let cursor_col = match cursor {
                        Some((cr, cc)) if cr == index => Some(cc),
                        _ => None,
                    };
                    line_from_cells(term.line(index), cursor_col)
                })
                .collect();

            content_card(
                // Clip is inside the padding: even when shrunk, the card
                // keeps its border and bottom margin visible.
                div()
                    .relative()
                    .flex_1()
                    .w_full()
                    .min_w(px(0.))
                    .min_h(px(0.))
                    .overflow_hidden()
                    .cursor_text()
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::on_terminal_mouse_down))
                    .on_mouse_move(cx.listener(Self::on_terminal_mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_terminal_mouse_up))
                    .on_scroll_wheel(cx.listener(Self::on_terminal_scroll))
                    .font_family(FONT)
                    .text_sm()
                    .children(lines)
                    .children(self.selection_overlays(term)),
            )
        };

        let body = match self.tabs_position {
            TabPosition::Top => div()
                .flex_1()
                .w_full()
                .min_w(px(0.))
                .min_h(px(0.))
                .flex()
                .flex_col()
                .pl(theme::GAP)
                .pr(theme::GAP)
                .pb(theme::GAP)
                .child(card),
            TabPosition::Bottom => div()
                .flex_1()
                .w_full()
                .min_w(px(0.))
                .min_h(px(0.))
                .flex()
                .flex_col()
                .gap(theme::GAP)
                .pl(theme::GAP)
                .pr(theme::GAP)
                .pb(theme::GAP)
                .child(card)
                .child(self.bottom_tabs(cx)),
            TabPosition::Left => div()
                .flex_1()
                .w_full()
                .min_w(px(0.))
                .min_h(px(0.))
                .flex()
                .gap(theme::GAP)
                .pl(theme::GAP)
                .pr(theme::GAP)
                .pb(theme::GAP)
                .child(self.side_tabs(cx))
                .child(card),
            TabPosition::Right => div()
                .flex_1()
                .w_full()
                .min_w(px(0.))
                .min_h(px(0.))
                .flex()
                .gap(theme::GAP)
                .pl(theme::GAP)
                .pr(theme::GAP)
                .pb(theme::GAP)
                .child(card)
                .child(self.side_tabs(cx)),
        };

        // Il focus deve stare nel body, non nella root: su Windows un elemento
        // con track_focus chiama prevent_default() su ogni mouse down e
        // GPUI 0.2.2 scarta WM_NCLBUTTONDOWN (HTCAPTION), bloccando il drag
        // della titlebar client-side.
        let body = body.track_focus(&self.focus);

        let root = div()
            .on_key_down(cx.listener(Self::on_key))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .rounded(theme::WINDOW_RADIUS)
            .bg(color_of(theme::CHROME))
            .whitespace_nowrap()
            .overflow_hidden()
            .child(self.chrome(window, cx))
            .child(body);

        #[cfg(any(target_os = "linux", target_os = "freebsd"))]
        let root = root.child(self.resize_zones());

        root
    }
}

fn main() {
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    WINDOW_SIZE,
                    cx,
                ))),
                titlebar: Some(TitlebarOptions {
                    title: Some("ftty".into()),
                    appears_transparent: true,
                    traffic_light_position: if cfg!(target_os = "macos") {
                        Some(point(px(12.), px(8.)))
                    } else {
                        None
                    },
                }),
                window_decorations: Some(WindowDecorations::Client),
                window_background: WindowBackgroundAppearance::Transparent,
                window_min_size: Some(MIN_WINDOW_SIZE),
                ..Default::default()
            };
            cx.open_window(options, |window, cx| {
                cx.new(|cx| TerminalView::new(window, cx))
            })
            .unwrap();
            cx.activate(true);
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use term::Color;

    fn cells(text: &str, width: usize) -> Vec<Cell> {
        let mut cells = vec![Cell::default(); width];
        for (cell, ch) in cells.iter_mut().zip(text.chars()) {
            cell.ch = ch;
        }
        cells
    }

    // `use gpui::*` also imports gpui's `test` macro: explicit path.
    #[::core::prelude::v1::test]
    fn minimum_size_depends_on_tab_position() {
        use TabPosition::*;
        assert_eq!(layout_min_size(Top), MIN_WINDOW_SIZE);
        assert_eq!(layout_min_size(Bottom).width, MIN_WINDOW_SIZE.width);
        assert_eq!(
            layout_min_size(Bottom).height,
            MIN_WINDOW_SIZE.height + theme::TAB_HEIGHT + theme::GAP
        );
        assert_eq!(
            layout_min_size(Left).width,
            MIN_WINDOW_SIZE.width + theme::STRIP_WIDTH + theme::GAP
        );
        assert_eq!(layout_min_size(Left).height, MIN_WINDOW_SIZE.height);
    }

    #[::core::prelude::v1::test]
    fn layout_change_keeps_terminal_size() {
        use TabPosition::*;
        assert_eq!(
            layout_size_delta(Top, Bottom),
            (px(0.), theme::TAB_HEIGHT + theme::GAP)
        );
        assert_eq!(
            layout_size_delta(Bottom, Top),
            (px(0.), -(theme::TAB_HEIGHT + theme::GAP))
        );
        assert_eq!(
            layout_size_delta(Top, Left),
            (theme::STRIP_WIDTH + theme::GAP, px(0.))
        );
        assert_eq!(layout_size_delta(Left, Right), (px(0.), px(0.)));
        assert_eq!(
            layout_size_delta(Right, Bottom),
            (
                -(theme::STRIP_WIDTH + theme::GAP),
                theme::TAB_HEIGHT + theme::GAP
            )
        );
    }

    #[::core::prelude::v1::test]
    fn active_index_survives_tab_close() {
        // Close a tab on the left: the active index shifts by one.
        assert_eq!(active_after_close(2, 0, 3), 1);
        // Close the active tab in the middle: it stays at the same position (next one).
        assert_eq!(active_after_close(1, 1, 2), 1);
        // Close the last active tab: switch to the previous one.
        assert_eq!(active_after_close(2, 2, 2), 1);
        assert_eq!(active_after_close(0, 0, 1), 0);
    }

    fn line_text(cells: &[Cell]) -> String {
        cells
            .iter()
            .map(|cell| cell.ch)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    #[::core::prelude::v1::test]
    fn scroll_offset_clamps_and_accumulates() {
        let mut acc = 0.;
        assert_eq!(apply_scroll(0, 10, 2., &mut acc), 2);
        assert_eq!(apply_scroll(2, 10, -5., &mut acc), 0);
        assert_eq!(apply_scroll(0, 10, 1.5, &mut acc), 1);
        assert_eq!(apply_scroll(1, 10, 0.6, &mut acc), 2);
        assert_eq!(apply_scroll(2, 10, 100., &mut acc), 10);
    }

    #[::core::prelude::v1::test]
    fn conpty_dsr_is_detected() {
        let mut emu = Emulator::new(8, 2);
        emu.feed(b"\x1b[6n");
        assert!(emu.term.dsr_pending);
    }

    #[::core::prelude::v1::test]
    fn scrollback_exposes_old_rows() {
        let mut emu = Emulator::new(8, 2);
        emu.feed(b"one\r\ntwo\r\nthree\r\n");
        let term = &emu.term;
        assert_eq!(term.history_len(), 2);
        assert_eq!(line_text(term.line(0)), "one");
        assert_eq!(line_text(term.line(1)), "two");
        assert_eq!(line_text(term.line(2)), "three");
        assert_eq!(term.visible_top(0), 2);
        assert_eq!(term.visible_top(1), 1);
        assert_eq!(term.visible_top(99), 0);
    }

    #[::core::prelude::v1::test]
    fn scrollback_is_capped() {
        let mut emu = Emulator::new(4, 1);
        for _ in 0..(term::MAX_SCROLLBACK + 5) {
            emu.feed(b"x\r\n");
        }
        assert_eq!(emu.term.history_len(), term::MAX_SCROLLBACK);
    }

    #[::core::prelude::v1::test]
    fn selection_text_joins_rows() {
        let mut term = Term::new(4, 3);
        for (cell, ch) in term.cells.iter_mut().zip("abcdefghijkl".chars()) {
            cell.ch = ch;
        }
        assert_eq!(selection_text(&term, (0, 1), (1, 2)), "bcd\nefg");
        assert_eq!(selection_text(&term, (2, 2), (1, 1)), "fgh\nijk");
    }

    #[::core::prelude::v1::test]
    fn line_len_trims_only_trailing_default_cells() {
        assert_eq!(line_len(&cells("", 8), None), 0);
        assert_eq!(line_len(&cells("ab", 8), None), 2);

        // A cell with a colored background is not "empty" and stays visible.
        let mut c = cells("ab", 8);
        c[5].style.bg = Color::Indexed(1);
        assert_eq!(line_len(&c, None), 6);

        // The cursor past the text is not trimmed.
        assert_eq!(line_len(&c, Some(7)), 8);
    }
}
