//! Alacritty owns terminal state. This adapter exports bounded VT repaint data.
//! No private xterm internals are patched. Full fidelity remains a P0/P2 gate:
//! palette changes, charset state, cursor shape and obscure VT modes need golden
//! comparisons against actual Codex builds. The API/UI reports this explicitly.
use alacritty_terminal::{
    event::{Event, EventListener},
    grid::Dimensions,
    index::{Column, Line},
    term::{
        cell::{Cell, Flags},
        Config, Osc52, TermMode,
    },
    vte::ansi::{CharsetIndex, Color, CursorShape, Processor, Rgb, StandardCharset},
    Grid, Term,
};
use crossbeam_channel::Sender;
use std::fmt::Write as _;
#[derive(Clone)]
struct Events(Sender<Vec<u8>>);
impl EventListener for Events {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(s) => {
                let _ = self.0.try_send(s.into_bytes());
            }
            Event::ColorRequest(index, format) => {
                let rgb = if index == 257 {
                    Rgb {
                        r: 15,
                        g: 20,
                        b: 28,
                    }
                } else {
                    Rgb {
                        r: 219,
                        g: 228,
                        b: 240,
                    }
                };
                let _ = self.0.try_send(format(rgb).into_bytes());
            }
            // Clipboard events are deliberately not executed. No OS clipboard access.
            _ => {}
        }
    }
}
#[derive(Clone, Copy)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}
impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }
    fn screen_lines(&self) -> usize {
        self.rows as usize
    }
    fn columns(&self) -> usize {
        self.cols as usize
    }
}
pub struct TerminalModel {
    term: Term<Events>,
    parser: Processor,
    size: Size,
    pub warnings: Vec<String>,
}
impl TerminalModel {
    pub fn new(cols: u16, rows: u16, replies: Sender<Vec<u8>>) -> Self {
        let size = Size { cols, rows };
        let config = Config {
            scrolling_history: 2000,
            osc52: Osc52::Disabled,
            kitty_keyboard: false,
            ..Default::default()
        };
        Self{term:Term::new(config,&size,Events(replies)),parser:Processor::new(),size,warnings:vec!["VT snapshot adapter is experimental; Codex/IME/full-state golden verification is not yet complete".into()]}
    }
    pub fn apply(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
        // Only rendering clients need synchronized-update batching. Keep the headless
        // canonical model current so a snapshot's output_seq never outruns its grid.
        // Original bytes still go unchanged to xterm, which keeps its visual batching.
        if bytes == b"\x1b[?2026h" || self.parser.sync_bytes_count() > 0 {
            self.parser.stop_sync(&mut self.term);
        }
    }
    pub fn resize(&mut self, size: Size) {
        self.term.resize(size);
        self.size = size;
    }
    pub fn size(&self) -> Size {
        self.size
    }
    pub fn bracketed_paste(&self) -> bool {
        self.term.mode().contains(TermMode::BRACKETED_PASTE)
    }
    pub fn readable_projection(&self) -> serde_json::Value {
        use serde_json::json;
        if self.term.mode().contains(TermMode::ALT_SCREEN) {
            return json!({"source":"terminal_raw","reason":"alternate_screen"});
        }
        let grid = self.term.grid();
        let screen_lines = grid.screen_lines();
        let columns = grid.columns();
        if screen_lines > 150 || columns > 400 {
            return json!({"source":"terminal_raw","reason":"limit"});
        }
        let mut utf8_bytes: usize = 0;
        let mut lines: Vec<String> = Vec::with_capacity(screen_lines);
        for row in 0..screen_lines {
            let mut row_chars: Vec<char> = Vec::with_capacity(columns);
            for col in 0..columns {
                let cell = &grid[Line(row as i32)][Column(col)];
                let flags = cell.flags;
                if flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                    continue;
                }
                if flags.contains(Flags::HIDDEN) {
                    let spaces = if flags.contains(Flags::WIDE_CHAR) {
                        2
                    } else {
                        1
                    };
                    if utf8_bytes + spaces > 256 * 1024 {
                        return serde_json::json!({"source":"terminal_raw","reason":"limit"});
                    }
                    utf8_bytes += spaces;
                    row_chars.extend(std::iter::repeat_n(' ', spaces));
                    continue;
                }
                if !cell.c.is_control() {
                    let mut buf = [0u8; 4];
                    let s = cell.c.encode_utf8(&mut buf);
                    utf8_bytes += s.len();
                    if utf8_bytes > 256 * 1024 {
                        return json!({"source":"terminal_raw","reason":"limit"});
                    }
                    row_chars.push(cell.c);
                } else {
                    utf8_bytes += 1;
                    if utf8_bytes > 256 * 1024 {
                        return json!({"source":"terminal_raw","reason":"limit"});
                    }
                    row_chars.push(' ');
                }
                for zw in cell.zerowidth().into_iter().flatten() {
                    if !zw.is_control() {
                        let mut buf = [0u8; 4];
                        let s = zw.encode_utf8(&mut buf);
                        utf8_bytes += s.len();
                        if utf8_bytes > 256 * 1024 {
                            return json!({"source":"terminal_raw","reason":"limit"});
                        }
                        row_chars.push(*zw);
                    }
                }
            }
            let mut s: String = row_chars.iter().collect();
            let trimmed_len = s.trim_end_matches(' ').len();
            s.truncate(trimmed_len);
            lines.push(s);
        }
        json!({"source":"terminal_projection","scope":"current_screen","lines":lines,"truncated":false})
    }
    pub fn snapshot(&self) -> Vec<u8> {
        let mut s = String::from("\x1bc\x1b[?25l\x1b[?7h\x1b[?6l\x1b[0m");
        let mode = self.term.mode();
        if mode.contains(TermMode::ALT_SCREEN) {
            let primary = self.term.inactive_grid();
            paint_grid(primary, &mut s);
            restore_cursor(primary, &mut s);
            s.push_str("\x1b[?1049h");
        }
        emit_ascii_charsets(&mut s);
        paint_grid(self.term.grid(), &mut s);
        s.push_str("\x1b[3g");
        for col in self.term.tab_stops() {
            let _ = write!(s, "\x1b[1;{}H\x1bH", col.0 + 1);
        }
        let region = self.term.scroll_region();
        let _ = write!(s, "\x1b[{};{}r", region.start.0 + 1, region.end.0);
        let cursor_style = self.term.cursor_style();
        for (flag, n) in [
            (TermMode::APP_CURSOR, 1),
            (TermMode::SHOW_CURSOR, 25),
            (TermMode::LINE_WRAP, 7),
            (TermMode::MOUSE_REPORT_CLICK, 1000),
            (TermMode::MOUSE_DRAG, 1002),
            (TermMode::MOUSE_MOTION, 1003),
            (TermMode::FOCUS_IN_OUT, 1004),
            (TermMode::UTF8_MOUSE, 1005),
            (TermMode::SGR_MOUSE, 1006),
            (TermMode::BRACKETED_PASTE, 2004),
        ] {
            let _ = write!(
                s,
                "\x1b[?{}{}",
                n,
                if flag == TermMode::SHOW_CURSOR && cursor_style.shape == CursorShape::Hidden {
                    'l'
                } else if mode.contains(flag) {
                    'h'
                } else {
                    'l'
                }
            );
        }
        if cursor_style.shape != CursorShape::Hidden {
            let code = match (cursor_style.shape, cursor_style.blinking) {
                (CursorShape::Block, true) => 1,
                (CursorShape::Block, false) => 2,
                (CursorShape::Underline, true) => 3,
                (CursorShape::Underline, false) => 4,
                (CursorShape::Beam, true) => 5,
                (CursorShape::Beam, false) => 6,
                (CursorShape::HollowBlock, true) => 1,
                (CursorShape::HollowBlock, false) => 2,
                (CursorShape::Hidden, _) => unreachable!(),
            };
            let _ = write!(s, "\x1b[{} q", code);
        }
        s.push_str(if mode.contains(TermMode::APP_KEYPAD) {
            "\x1b="
        } else {
            "\x1b>"
        });
        s.push_str(if mode.contains(TermMode::INSERT) {
            "\x1b[4h"
        } else {
            "\x1b[4l"
        });
        s.push_str(if mode.contains(TermMode::LINE_FEED_NEW_LINE) {
            "\x1b[20h"
        } else {
            "\x1b[20l"
        });
        restore_cursor(self.term.grid(), &mut s);
        if mode.contains(TermMode::ORIGIN) {
            s.push_str("\x1b[?6h");
            restore_one(
                self.term.grid(),
                &self.term.grid().cursor,
                &mut s,
                self.term.scroll_region().start.0,
            );
        }
        let colors = self.term.renderable_content().colors;
        for index in 0..=258usize {
            if let Some(c) = colors[index] {
                if index < 256 {
                    let _ = write!(
                        s,
                        "\x1b]4;{};rgb:{:02x}/{:02x}/{:02x}\x07",
                        index, c.r, c.g, c.b
                    );
                } else {
                    let _ = write!(
                        s,
                        "\x1b]{};rgb:{:02x}/{:02x}/{:02x}\x07",
                        index - 246,
                        c.r,
                        c.g,
                        c.b
                    );
                }
            }
        }
        s.into_bytes()
    }
}
fn color(c: Color, fg: bool, out: &mut String) {
    match c {
        Color::Spec(rgb) => {
            let _ = write!(
                out,
                ";{};2;{};{};{}",
                if fg { 38 } else { 48 },
                rgb.r,
                rgb.g,
                rgb.b
            );
        }
        Color::Indexed(n) => {
            let _ = write!(out, ";{};5;{}", if fg { 38 } else { 48 }, n);
        }
        Color::Named(n) => {
            let n = n as u16;
            if n < 16 {
                let _ = write!(out, ";{};5;{}", if fg { 38 } else { 48 }, n);
            } else {
                let _ = write!(out, ";{}", if fg { 39 } else { 49 });
            }
        }
    }
}
fn style(c: &Cell) -> String {
    let mut s = String::from("\x1b[0");
    color(c.fg, true, &mut s);
    color(c.bg, false, &mut s);
    for (flag, n) in [
        (Flags::BOLD, 1),
        (Flags::DIM, 2),
        (Flags::ITALIC, 3),
        (Flags::INVERSE, 7),
        (Flags::HIDDEN, 8),
        (Flags::STRIKEOUT, 9),
    ] {
        if c.flags.contains(flag) {
            let _ = write!(s, ";{n}");
        }
    }
    let underline = [
        (Flags::DASHED_UNDERLINE, 5),
        (Flags::DOTTED_UNDERLINE, 4),
        (Flags::UNDERCURL, 3),
        (Flags::DOUBLE_UNDERLINE, 2),
        (Flags::UNDERLINE, 1),
    ];
    if let Some((_, code)) = underline
        .into_iter()
        .find(|(flag, _)| c.flags.contains(*flag))
    {
        // Underline variants use the colon subparameter form.  `4;1` means
        // underline followed by bold in SGR, which changes restored cell
        // attributes instead of selecting single underline.
        let _ = write!(s, ";4:{code}");
    }
    if let Some(underline) = c.underline_color() {
        match underline {
            Color::Spec(rgb) => {
                let _ = write!(s, ";58;2;{};{};{}", rgb.r, rgb.g, rgb.b);
            }
            Color::Indexed(n) => {
                let _ = write!(s, ";58;5;{}", n);
            }
            Color::Named(n) => {
                let _ = write!(s, ";58;5;{}", n as u16);
            }
        }
    }
    s.push('m');
    s
}
fn paint_grid(grid: &Grid<Cell>, out: &mut String) {
    out.push_str("\x1b[H\x1b[2J\x1b[?7h\x1b[?6l\x1b[r");
    let start = -(grid.history_size().min(2000) as i32);
    let end = grid.screen_lines() as i32;
    let mut last_style = String::new();
    for row in start..end {
        for col in 0..grid.columns() {
            let cell = &grid[Line(row)][Column(col)];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let st = style(cell);
            if st != last_style {
                out.push_str(&st);
                last_style = st;
            }
            out.push(if cell.c.is_control() { ' ' } else { cell.c });
            for c in cell.zerowidth().into_iter().flatten() {
                if !c.is_control() {
                    out.push(*c);
                }
            }
        }
        if row + 1 < end
            && !grid[Line(row)][Column(grid.columns() - 1)]
                .flags
                .contains(Flags::WRAPLINE)
        {
            out.push_str("\r\n");
        }
        if out.len() > 4 * 1024 * 1024 {
            break;
        } // Caller rejects oversized snapshots; never advertises a truncated one as complete.
    }
}
fn restore_cursor(grid: &Grid<Cell>, out: &mut String) {
    restore_one(grid, &grid.saved_cursor, out, 0);
    out.push_str("\x1b7");
    restore_one(grid, &grid.cursor, out, 0);
}

fn emit_charsets(charsets: &alacritty_terminal::grid::Charsets, out: &mut String) {
    for (index, intermediate) in [
        (CharsetIndex::G0, '('),
        (CharsetIndex::G1, ')'),
        (CharsetIndex::G2, '*'),
        (CharsetIndex::G3, '+'),
    ] {
        let designator = match charsets[index] {
            StandardCharset::Ascii => 'B',
            StandardCharset::SpecialCharacterAndLineDrawing => '0',
        };
        let _ = write!(out, "\x1b{}{}", intermediate, designator);
    }
}

fn emit_ascii_charsets(out: &mut String) {
    out.push_str("\x1b(B\x1b)B\x1b*B\x1b+B");
}
fn restore_one(
    grid: &Grid<Cell>,
    cursor: &alacritty_terminal::grid::Cursor<Cell>,
    out: &mut String,
    origin_top: i32,
) {
    emit_ascii_charsets(out);
    let row = cursor.point.line.0.clamp(0, grid.screen_lines() as i32 - 1);
    let mut col = cursor.point.column.0.min(grid.columns() - 1);
    // CUP clears pending wrap. Repaint the same final glyph to recreate delayed wrap
    // without moving data or depending on xterm's private screen internals.
    let previous_has_zero_width = col > 0
        && grid[Line(row)][Column(col - 1)]
            .zerowidth()
            .is_some_and(|chars| !chars.is_empty());
    if previous_has_zero_width {
        let previous = &grid[Line(row)][Column(col - 1)];
        let _ = write!(out, "\x1b[{};{}H", (row - origin_top + 1).max(1), col);
        out.push_str(&style(previous));
        out.push(if previous.c.is_control() {
            ' '
        } else {
            previous.c
        });
        for c in previous.zerowidth().into_iter().flatten() {
            if !c.is_control() {
                out.push(*c);
            }
        }
    } else if cursor.input_needs_wrap && col == grid.columns() - 1 {
        if col > 0
            && grid[Line(row)][Column(col)]
                .flags
                .contains(Flags::WIDE_CHAR_SPACER)
        {
            col -= 1;
        }
        let cell = &grid[Line(row)][Column(col)];
        let _ = write!(out, "\x1b[{};{}H", (row - origin_top + 1).max(1), col + 1);
        out.push_str(&style(cell));
        out.push(if cell.c.is_control() { ' ' } else { cell.c });
        for c in cell.zerowidth().into_iter().flatten() {
            if !c.is_control() {
                out.push(*c);
            }
        }
    } else {
        let _ = write!(out, "\x1b[{};{}H", (row - origin_top + 1).max(1), col + 1);
    }
    emit_charsets(&cursor.charsets, out);
    match cursor.active_charset {
        CharsetIndex::G0 => out.push_str("\x1b[0m\x0f"),
        CharsetIndex::G1 => out.push_str("\x1b[0m\x0e"),
        // The VT profile currently snapshots G0/G1 selection only. G2/G3
        // designation is preserved and LS2/LS3 selection is intentionally
        // outside this adapter's supported profile.
        CharsetIndex::G2 | CharsetIndex::G3 => out.push_str("\x1b[0m\x0f"),
    }
    out.push_str(&style(&cursor.template));
    // Saved charset, saved origin/wrap modes and uncommon cursor attributes still
    // need the pinned-engine/xterm golden matrix; this is not full-state certification.
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn korean_snapshot_and_mouse_mode() {
        let (tx, _) = crossbeam_channel::bounded(128);
        let mut t = TerminalModel::new(80, 24, tx);
        t.apply("안녕하세요".as_bytes());
        t.apply(b"\x1b[?1006h");
        let s = String::from_utf8(t.snapshot()).unwrap();
        assert!(s.contains("안녕하세요"));
        assert!(s.contains("\x1b[?1006h"));
    }
    #[test]
    fn query_reply_has_single_server_owner() {
        let (tx, rx) = crossbeam_channel::bounded(128);
        let mut t = TerminalModel::new(80, 24, tx);
        t.apply(b"\x1b[6n");
        assert!(rx.try_recv().unwrap().starts_with(b"\x1b["));
    }

    fn model(cols: u16) -> TerminalModel {
        let (tx, _) = crossbeam_channel::bounded(128);
        TerminalModel::new(cols, 5, tx)
    }
    #[test]
    fn headless_sync_keeps_snapshot_seq_current() {
        let mut t = model(8);
        t.apply(b"\x1b[?2026h");
        t.apply(b"partial");
        assert_eq!(t.term.grid()[Line(0)][Column(0)].c, 'p');
        t.apply(b"\x1b[?2026l");
    }
    #[test]
    fn pending_wrap_survives_snapshot() {
        let mut t = model(4);
        t.apply(b"ABCD");
        assert!(t.term.grid().cursor.input_needs_wrap);
        let s = t.snapshot();
        let mut other = model(4);
        other.apply(&s);
        assert!(other.term.grid().cursor.input_needs_wrap);
        other.apply(b"E");
        assert_eq!(other.term.grid()[Line(1)][Column(0)].c, 'E');
    }
    #[test]
    fn palette_entries_are_exported() {
        let mut t = model(8);
        t.apply(b"\x1b]4;1;rgb:11/22/33\x07");
        let s = String::from_utf8(t.snapshot()).unwrap();
        assert!(s.contains("\x1b]4;1;rgb:11/22/33\x07"));
    }
    #[test]
    fn cursor_style_survives_snapshot() {
        for code in 1..=6 {
            let mut t = model(8);
            t.apply(format!("\x1b[{code} q").as_bytes());
            let s = t.snapshot();
            let mut other = model(8);
            other.apply(&s);
            assert_eq!(
                t.term.cursor_style(),
                other.term.cursor_style(),
                "DECSCUSR {code}"
            );
        }
    }
    #[test]
    fn saved_line_drawing_charset_survives_snapshot() {
        let mut source = model(8);
        source.apply(b"\x1b(0");
        source.apply(b"\x1b7");
        source.apply(b"\x1b(B");
        let snapshot = source.snapshot();
        let mut other = model(8);
        other.apply(&snapshot);

        source.apply(b"\x1b8q");
        other.apply(b"\x1b8q");
        assert_eq!(source.term.grid()[Line(0)][Column(0)].c, '\u{2500}');
        assert_eq!(
            other.term.grid()[Line(0)][Column(0)].c,
            source.term.grid()[Line(0)][Column(0)].c
        );
    }
    #[test]
    fn pending_wrap_literal_is_not_remapped_by_charset_snapshot() {
        let mut source = model(4);
        source.apply(b"abcq\x1b(0");
        let snapshot = source.snapshot();
        let mut other = model(4);
        other.apply(&snapshot);
        assert_eq!(source.term.grid()[Line(0)][Column(3)].c, 'q');
        assert_eq!(other.term.grid()[Line(0)][Column(3)].c, 'q');
    }
    #[test]
    fn coalesced_alt_entry_preserves_primary_in_snapshot() {
        let (tx, _rx) = crossbeam_channel::bounded(128);
        let mut source = TerminalModel::new(8, 4, tx.clone());
        let mut restored = TerminalModel::new(8, 4, tx);
        source.apply(b"P");
        source.apply(b"\x1b[?1049hA");
        restored.apply(&source.snapshot());
        restored.apply(b"\x1b[?1049l");
        source.apply(b"\x1b[?1049l");
        assert_eq!(source.term.grid()[Line(0)][Column(0)].c, 'P');
        assert_eq!(restored.term.grid()[Line(0)][Column(0)].c, 'P');
    }

    #[test]
    fn split_alt_entry_preserves_primary_in_snapshot() {
        let (tx, _rx) = crossbeam_channel::bounded(128);
        let mut source = TerminalModel::new(8, 4, tx.clone());
        let mut restored = TerminalModel::new(8, 4, tx);
        source.apply(b"P");
        source.apply(b"\x1b[?1049h");
        source.apply(b"A");
        restored.apply(&source.snapshot());
        restored.apply(b"\x1b[?1049l");
        source.apply(b"\x1b[?1049l");
        assert_eq!(source.term.grid()[Line(0)][Column(0)].c, 'P');
        assert_eq!(restored.term.grid()[Line(0)][Column(0)].c, 'P');
    }

    #[test]
    fn charset_and_underline_state_continue_after_snapshot() {
        let mut source = model(12);
        source.apply(b"\x1b(0\x1b7\x1b(B\x1b[4:3m\x1b[58;5;42mX");
        let snapshot = source.snapshot();
        let mut restored = model(12);
        restored.apply(&snapshot);
        source.apply(b"\x1b8q");
        restored.apply(b"\x1b8q");
        assert_eq!(source.term.grid()[Line(0)][Column(0)].c, '\u{2500}');
        assert_eq!(restored.term.grid()[Line(0)][Column(0)].c, '\u{2500}');
        assert_eq!(
            source.term.grid()[Line(0)][Column(0)].underline_color(),
            restored.term.grid()[Line(0)][Column(0)].underline_color()
        );
    }
    #[test]
    fn coalesced_tab_controls_round_trip_snapshot() {
        let (tx, _rx) = crossbeam_channel::bounded(128);
        let mut source = TerminalModel::new(12, 4, tx.clone());
        let mut restored = TerminalModel::new(12, 4, tx);
        source.apply(b"\x1b[3g\x1b[4G\x1bH\r");
        restored.apply(&source.snapshot());
        restored.apply(b"\tX");
        source.apply(b"\tX");
        assert_eq!(source.term.grid()[Line(0)][Column(3)].c, 'X');
        assert_eq!(restored.term.grid()[Line(0)][Column(3)].c, 'X');
    }
    #[test]
    fn projection_contains_expected_glyphs_and_no_secret() {
        let mut model = model(80);
        model.apply("한글 e\u{301} 🙂\x1b[8mSECRET界\x1b[0m visible".as_bytes());
        let value = model.readable_projection();
        let obj = value.as_object().expect("projection object");
        assert_eq!(
            obj.get("source").and_then(|v| v.as_str()),
            Some("terminal_projection")
        );
        assert_eq!(
            obj.get("scope").and_then(|v| v.as_str()),
            Some("current_screen")
        );
        let lines = obj
            .get("lines")
            .and_then(|v| v.as_array())
            .expect("lines array");
        let line0 = lines[0].as_str().expect("line0 string");
        assert!(line0.contains("한글"), "expected Korean in line0: {line0}");
        assert!(
            line0.contains("e\u{301}"),
            "expected combining sequence in line0: {line0}"
        );
        assert!(line0.contains("🙂"), "expected emoji in line0: {line0}");
        assert!(
            line0.contains("visible"),
            "expected visible in line0: {line0}"
        );
        assert!(!line0.contains("SECRET"), "SECRET must be hidden: {line0}");
        assert!(!line0.contains("界"), "hidden glyph must not leak: {line0}");
        assert!(
            !line0.chars().any(|c| c.is_control()),
            "line0 must not contain control characters: {line0}"
        );
    }

    #[test]
    fn cr_replaces_line_in_place_not_appended_log() {
        let mut model = model(80);
        model.apply(b"progress 10%\rprogress 99%");
        let value = model.readable_projection();
        let obj = value.as_object().expect("projection object");
        let lines = obj
            .get("lines")
            .and_then(|v| v.as_array())
            .expect("lines array");
        let line0 = lines[0].as_str().expect("line0 string");
        assert!(
            line0.starts_with("progress 99%"),
            "line0 should start with progress 99%, got: {line0}"
        );
        assert!(
            !line0.contains("10%"),
            "line0 must not contain prior progress 10%, got: {line0}"
        );
        assert_eq!(
            lines.len(),
            5,
            "replaced grid must not append a log entry, got {} lines",
            lines.len()
        );
    }

    #[test]
    fn alternate_screen_round_trip_restores_primary_screen() {
        let mut model = model(80);
        model.apply(b"primary\x1b[?1049hsecondary");
        let raw = model.readable_projection();
        let raw_obj = raw.as_object().expect("raw projection object");
        assert_eq!(
            raw_obj.get("source").and_then(|v| v.as_str()),
            Some("terminal_raw")
        );
        assert_eq!(
            raw_obj.get("reason").and_then(|v| v.as_str()),
            Some("alternate_screen")
        );
        model.apply(b"\x1b[?1049l");
        let value = model.readable_projection();
        let obj = value.as_object().expect("projection object");
        let lines = obj
            .get("lines")
            .and_then(|v| v.as_array())
            .expect("lines array");
        let line0 = lines[0].as_str().expect("line0 string");
        assert!(
            line0.starts_with("primary"),
            "line0 should start with primary after leaving alt screen, got: {line0}"
        );
    }

    #[test]
    fn wide_terminal_reports_width_limit_and_html_is_literal() {
        let wide = model(401);
        let raw = wide.readable_projection();
        let raw_obj = raw.as_object().expect("wide raw projection object");
        assert_eq!(
            raw_obj.get("reason").and_then(|v| v.as_str()),
            Some("limit")
        );

        let mut model = model(80);
        model.apply(b"<script>alert(1)</script>");
        let value = model.readable_projection();
        let obj = value.as_object().expect("projection object");
        let lines = obj
            .get("lines")
            .and_then(|v| v.as_array())
            .expect("lines array");
        let line0 = lines[0].as_str().expect("line0 string");
        assert_eq!(
            line0, "<script>alert(1)</script>",
            "line0 must preserve HTML literally, got: {line0}"
        );
        assert!(
            !line0.contains('<') || line0.contains("<script>"),
            "literal string preserved without HTML parsing"
        );
    }
}
