use super::*;

fn line_text(cells: &[Cell]) -> String {
    cells
        .iter()
        .map(|cell| cell.ch)
        .collect::<String>()
        .trim_end()
        .to_string()
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
    for _ in 0..(MAX_SCROLLBACK + 5) {
        emu.feed(b"x\r\n");
    }
    assert_eq!(emu.term.history_len(), MAX_SCROLLBACK);
}

#[::core::prelude::v1::test]
fn resize_wider_joins_soft_wrapped_lines() {
    let mut emu = Emulator::new(4, 3);
    emu.feed(b"abcdefgh");
    assert_eq!(line_text(emu.term.row(0)), "abcd");
    assert_eq!(line_text(emu.term.row(1)), "efgh");

    emu.term.resize(8, 3);
    assert_eq!(line_text(emu.term.row(0)), "abcdefgh");
    assert_eq!(emu.term.cursor, (0, 7));
}

#[::core::prelude::v1::test]
fn resize_narrower_splits_lines() {
    let mut emu = Emulator::new(8, 3);
    emu.feed(b"abcdefgh\r\nX");
    assert_eq!(line_text(emu.term.row(0)), "abcdefgh");

    emu.term.resize(4, 3);
    assert_eq!(line_text(emu.term.row(0)), "abcd");
    assert_eq!(line_text(emu.term.row(1)), "efgh");
    assert_eq!(line_text(emu.term.row(2)), "X");
    assert_eq!(emu.term.cursor, (2, 1));
}

#[::core::prelude::v1::test]
fn resize_keeps_explicit_line_breaks() {
    let mut emu = Emulator::new(4, 3);
    emu.feed(b"ab\r\ncd");
    emu.term.resize(2, 3);
    assert_eq!(line_text(emu.term.row(0)), "ab");
    assert_eq!(line_text(emu.term.row(1)), "cd");
}

#[::core::prelude::v1::test]
fn resize_reflows_scrollback_without_losing_it() {
    let mut emu = Emulator::new(4, 2);
    emu.feed(b"abcdefgh\r\nline2\r\nline3\r\n");

    emu.term.resize(8, 2);
    assert_eq!(emu.term.history_len(), 1);
    assert_eq!(line_text(emu.term.line(0)), "abcdefgh");
    assert_eq!(line_text(emu.term.line(1)), "line2");
    assert_eq!(line_text(emu.term.line(2)), "line3");
}

#[::core::prelude::v1::test]
fn osc_0_and_2_set_title() {
    let mut emu = Emulator::new(8, 2);
    emu.feed(b"\x1b]0;first title\x07");
    assert_eq!(emu.term.display_title(1), "first title");

    emu.feed(b"\x1b]2;second title\x1b\\");
    assert_eq!(emu.term.display_title(1), "second title");
}

#[::core::prelude::v1::test]
fn osc_0_with_icon_and_title_uses_title() {
    let mut emu = Emulator::new(8, 2);
    emu.feed(b"\x1b]0;icon;window title\x07");
    assert_eq!(emu.term.display_title(1), "window title");
}

#[::core::prelude::v1::test]
fn osc_7_extracts_directory_name() {
    let mut emu = Emulator::new(8, 2);
    emu.feed(b"\x1b]7;file:///C:/Users/DANIELE/Desktop/f-tty\x07");
    assert_eq!(
        emu.term.cwd.as_deref(),
        Some("C:/Users/DANIELE/Desktop/f-tty")
    );
    assert_eq!(emu.term.display_title(1), "f-tty");
}

#[::core::prelude::v1::test]
fn osc_7_decodes_percent_encoded_path() {
    let mut emu = Emulator::new(8, 2);
    emu.feed(b"\x1b]7;file://host/home/user/my%20projects/demo\x07");
    assert_eq!(emu.term.cwd.as_deref(), Some("/home/user/my projects/demo"));
    assert_eq!(emu.term.display_title(1), "demo");
}

#[::core::prelude::v1::test]
fn title_falls_back_to_cwd_then_default() {
    let mut emu = Emulator::new(8, 2);
    assert_eq!(emu.term.display_title(7), "Terminal 7");

    emu.term.cwd = Some("/var/log".into());
    assert_eq!(emu.term.display_title(7), "log");

    emu.term.title = Some("   ".into());
    assert_eq!(emu.term.display_title(7), "log");

    emu.term.title = Some("vim".into());
    assert_eq!(emu.term.display_title(7), "vim");
}
