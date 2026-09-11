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
    vte::ansi::{Color, Processor, Rgb},
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
    primary: Option<Grid<Cell>>,
    region: (u16, u16),
    tabs: Vec<u16>,
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
        Self{term:Term::new(config,&size,Events(replies)),parser:Processor::new(),size,primary:None,region:(1,rows),tabs:(8..cols).step_by(8).collect(),warnings:vec!["VT snapshot adapter is experimental; Codex/IME/full-state golden verification is not yet complete".into()]}
    }
    pub fn apply(&mut self, bytes: &[u8]) {
        let was_alt = self.term.mode().contains(TermMode::ALT_SCREEN);
        let candidate = if !was_alt && bytes.starts_with(b"\x1b[?") && bytes.last() == Some(&b'h') {
            let p = String::from_utf8_lossy(&bytes[3..bytes.len() - 1]);
            if p.split(';').any(|x| matches!(x, "47" | "1047" | "1049")) {
                let mut g = self.term.grid().clone();
                g.saved_cursor = g.cursor.clone();
                Some(g)
            } else {
                None
            }
        } else {
            None
        };
        if bytes == b"\x1bH" {
            let col = self.term.grid().cursor.point.column.0 as u16;
            if !self.tabs.contains(&col) {
                self.tabs.push(col);
                self.tabs.sort_unstable();
            }
        }
        if bytes == b"\x1b[3g" {
            self.tabs.clear();
        }
        if bytes == b"\x1b[g" || bytes == b"\x1b[0g" {
            let c = self.term.grid().cursor.point.column.0 as u16;
            self.tabs.retain(|x| *x != c);
        }
        if bytes.starts_with(b"\x1b[")
            && bytes.last() == Some(&b'r')
            && !bytes.starts_with(b"\x1b[?")
        {
            let text = String::from_utf8_lossy(&bytes[2..bytes.len() - 1]);
            let mut p = text.split(';');
            let a = p
                .next()
                .and_then(|x| x.parse::<u16>().ok())
                .unwrap_or(1)
                .max(1);
            let b = p
                .next()
                .and_then(|x| x.parse::<u16>().ok())
                .unwrap_or(self.size.rows)
                .min(self.size.rows);
            if a < b {
                self.region = (a, b);
            }
        }
        self.parser.advance(&mut self.term, bytes);
        // Only rendering clients need synchronized-update batching. Keep the headless
        // canonical model current so a snapshot's output_seq never outruns its grid.
        // Original bytes still go unchanged to xterm, which keeps its visual batching.
        if bytes == b"\x1b[?2026h" || self.parser.sync_bytes_count() > 0 {
            self.parser.stop_sync(&mut self.term);
        }
        let alt = self.term.mode().contains(TermMode::ALT_SCREEN);
        if !was_alt && alt {
            self.primary = candidate;
        }
        if was_alt && !alt {
            self.primary = None;
        }
        if bytes == b"\x1bc" {
            self.primary = None;
            self.region = (1, self.size.rows);
            self.tabs = (8..self.size.cols).step_by(8).collect();
        }
    }
    pub fn resize(&mut self, size: Size) {
        if size.cols != self.size.cols {
            self.tabs = (8..size.cols).step_by(8).collect();
        }
        if let Some(g) = &mut self.primary {
            g.resize(true, size.rows as usize, size.cols as usize);
        }
        self.term.resize(size);
        self.size = size;
        self.region = (1, size.rows);
    }
    pub fn size(&self) -> Size {
        self.size
    }
    pub fn bracketed_paste(&self) -> bool {
        self.term.mode().contains(TermMode::BRACKETED_PASTE)
    }
    pub fn snapshot(&self) -> Vec<u8> {
        let mut s = String::from("\x1bc\x1b[?25l\x1b[?7h\x1b[?6l\x1b[0m");
        if let Some(primary) = &self.primary {
            paint_grid(primary, &mut s);
            restore_cursor(primary, &mut s);
            s.push_str("\x1b[?1049h");
        } else if self.term.mode().contains(TermMode::ALT_SCREEN) {
            s.push_str("\x1b[?1049h");
        }
        paint_grid(self.term.grid(), &mut s);
        s.push_str("\x1b[3g");
        for col in &self.tabs {
            let _ = write!(s, "\x1b[1;{}H\x1bH", col + 1);
        }
        let _ = write!(s, "\x1b[{};{}r", self.region.0, self.region.1);
        let mode = self.term.mode();
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
                if mode.contains(flag) { 'h' } else { 'l' }
            );
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
                i32::from(self.region.0) - 1,
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
        (Flags::UNDERLINE, 4),
        (Flags::INVERSE, 7),
        (Flags::HIDDEN, 8),
        (Flags::STRIKEOUT, 9),
    ] {
        if c.flags.contains(flag) {
            let _ = write!(s, ";{n}");
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
fn restore_one(
    grid: &Grid<Cell>,
    cursor: &alacritty_terminal::grid::Cursor<Cell>,
    out: &mut String,
    origin_top: i32,
) {
    let row = cursor.point.line.0.clamp(0, grid.screen_lines() as i32 - 1);
    let mut col = cursor.point.column.0.min(grid.columns() - 1);
    // CUP clears pending wrap. Repaint the same final glyph to recreate delayed wrap
    // without moving data or depending on xterm's private screen internals.
    if cursor.input_needs_wrap && col == grid.columns() - 1 {
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
}
