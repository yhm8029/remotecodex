use std::char;

use serde_json::json;
use unicode_width::UnicodeWidthChar;

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--raw") {
        let mut widths: Vec<u8> = Vec::with_capacity(0x110000);
        for cp in 0u32..=0x10FFFF {
            widths.push(
                char::from_u32(cp)
                    .map(|c| UnicodeWidthChar::width(c).unwrap_or(0).min(2) as u8)
                    .unwrap_or(255),
            );
        }
        std::io::Write::write_all(&mut std::io::stdout().lock(), &widths)
            .expect("write scalar widths");
        return;
    }
    let mut ranges: Vec<(u32, u32, u8)> = Vec::new();
    let mut cur_start: Option<u32> = None;
    let mut cur_end: u32 = 0;
    let mut cur_width: u8 = 0;

    let flush = |start: u32, end: u32, width: u8, ranges: &mut Vec<(u32, u32, u8)>| {
        if width == 0 || width == 2 {
            ranges.push((start, end, width));
        }
    };

    for cp in 0u32..=0x10FFFF {
        if (0xD800..=0xDFFF).contains(&cp) {
            if let Some(s) = cur_start {
                flush(s, cur_end, cur_width, &mut ranges);
                cur_start = None;
            }
            continue;
        }
        let Some(c) = char::from_u32(cp) else {
            if let Some(s) = cur_start {
                flush(s, cur_end, cur_width, &mut ranges);
                cur_start = None;
            }
            continue;
        };
        let w = c.width().unwrap_or(0).min(2) as u8;
        let interesting = w == 0 || w == 2;
        match (cur_start, interesting) {
            (None, true) => {
                cur_start = Some(cp);
                cur_end = cp;
                cur_width = w;
            }
            (Some(_), true) => {
                if cur_width == w && cur_end + 1 == cp {
                    cur_end = cp;
                } else {
                    flush(cur_start.unwrap(), cur_end, cur_width, &mut ranges);
                    cur_start = Some(cp);
                    cur_end = cp;
                    cur_width = w;
                }
            }
            (Some(s), false) => {
                flush(s, cur_end, cur_width, &mut ranges);
                cur_start = None;
            }
            (None, false) => {}
        }
    }
    if let Some(s) = cur_start {
        flush(s, cur_end, cur_width, &mut ranges);
    }

    println!(
        "{}",
        json!({
            "unicode_version": unicode_width::UNICODE_VERSION,
            "crate_version": "0.2.2",
            "default_width": 1,
            "ranges": ranges,
        })
    );
}
