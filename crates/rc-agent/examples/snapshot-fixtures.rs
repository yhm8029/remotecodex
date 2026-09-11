#[path = "../src/terminal.rs"]
mod terminal;

use crossbeam_channel::bounded;
use serde::Serialize;
use terminal::TerminalModel;

#[derive(Serialize)]
struct Fixture {
    name: &'static str,
    cols: u16,
    rows: u16,
    prefix_chunks: Vec<String>,
    snapshot: String,
    continuation: String,
}

fn fixture(
    name: &'static str,
    cols: u16,
    rows: u16,
    prefix_chunks: &[&str],
    continuation: &'static str,
) -> Fixture {
    let (tx, _) = bounded(128);
    let mut model = TerminalModel::new(cols, rows, tx);
    for chunk in prefix_chunks {
        model.apply(chunk.as_bytes());
    }
    Fixture {
        name,
        cols,
        rows,
        prefix_chunks: prefix_chunks.iter().map(|chunk| (*chunk).into()).collect(),
        snapshot: String::from_utf8(model.snapshot()).expect("terminal snapshot is UTF-8"),
        continuation: continuation.into(),
    }
}

fn main() {
    let fixtures = vec![
        fixture("two-cell cap normal", 10, 3, &["A\u{17d8}B"], "C"),
        fixture(
            "two-cell cap insert",
            10,
            3,
            &["ABCDE\r\x1b[4h", "\u{17d8}"],
            "Z",
        ),
        fixture("two-cell cap right margin", 4, 3, &["ABC", "\u{17d8}"], "Z"),
        fixture(
            "Unicode 17 scalar emoji and ambiguous",
            20,
            3,
            &["\u{1f642}\u{1fae0}\u{1fae9}·한", "e\u{0301}"],
            "Z",
        ),
        fixture(
            "alternate screen with primary restore",
            10,
            4,
            &["Primary", "\x1b[?1049h", "Alt", "\x1b[?25l"],
            "\x1b[?1049lAfter",
        ),
        fixture(
            "tabs scroll region and origin",
            12,
            5,
            &["\x1b[3g\x1b[4G\x1bH\r", "\x1b[2;5r\x1b[?6h", "\x1b[2;1H\tX"],
            "\x1b[?6lY",
        ),
        fixture(
            "G1 saved and active charset",
            10,
            4,
            &["\x1b)0\x0e\x1b7", "\x1b(B\x0f", "q"],
            "\x1b8q\x0fZ",
        ),
        fixture(
            "split private modes and alternate screen",
            10,
            4,
            &["P\x1b[?10", "49hA", "\x1b[?25", "l"],
            "\x1b[?1049lR",
        ),
        fixture("pending wrap", 4, 3, &["AB", "CD"], "E"),
        fixture("wrap disabled", 4, 3, &["\x1b[?7l", "ABCD"], "E"),
        fixture(
            "unicode width and combining",
            14,
            4,
            &["A界한", "\u{0301}", "e\u{0301}🙂"],
            "Z",
        ),
        fixture(
            "underline variants and indexed RGB",
            18,
            4,
            &[
                "\x1b[4:1mA\x1b[4:2mB\x1b[4:3mC\x1b[4:4mD\x1b[4:5mE",
                "\x1b[58;5;42mX\x1b[58;2;9;8;7mU\x1b[38;2;12;34;56mR",
            ],
            "Y",
        ),
    ];
    println!(
        "{}",
        serde_json::to_string(&fixtures).expect("serialize fixtures")
    );
}
