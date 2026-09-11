#![cfg(windows)]
//! Opt-in Windows process tests. Never silently skipped and counted as Windows PASS on Linux.
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::{
    io::{Read, Write},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[test]
#[ignore = "Requires actual Windows ConPTY. Run with --ignored --nocapture on the user's PC"]
fn cmd_real_pty_round_trip() {
    let pair = native_pty_system()
        .openpty(PtySize {
            cols: 120,
            rows: 32,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut command = CommandBuilder::new("cmd.exe");
    command.args(["/D", "/Q"]);
    let mut child = pair.slave.spawn_command(command).unwrap();
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();

    let (tx, rx) = mpsc::channel();
    let reader_handle = thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let marker = format!("RC_PTY_OK_{}", uuid::Uuid::new_v4());
    // The expected outputs do not appear verbatim in the input, preventing command-echo false positives.
    let expected = format!("{marker}_RESULT");
    let payload = format!(
        "@chcp 65001 >nul\r\n@set \"RC_MARK={marker}\"\r\n@echo %RC_MARK%_RESULT\r\n@set \"RC_KR=한글\"\r\n@echo %RC_KR%왕복\r\n"
    );

    let until = Instant::now() + Duration::from_secs(10);
    let mut all = Vec::new();
    let mut query_observed = false;
    let mut query_replied = false;
    let mut passed = false;

    // Phase 1: wait until we observe the DSR cursor query ESC[6n, then reply once.
    while Instant::now() < until {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(chunk) => {
                all.extend_from_slice(&chunk);
                if !query_replied && all.windows(4).any(|w| w == b"\x1b[6n") {
                    let _ = writer.write_all(b"\x1b[1;1R");
                    let _ = writer.flush();
                    query_observed = true;
                    query_replied = true;
                }
                if query_replied {
                    break;
                }
            }
            Err(_) => {}
        }
    }

    // Phase 2: send the command batch now that CMD is no longer blocked on the cursor query.
    let _ = writer.write_all(payload.as_bytes());
    let _ = writer.flush();

    // Phase 3: collect remaining output until success or deadline.
    while Instant::now() < until {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(chunk) => {
                all.extend_from_slice(&chunk);
                let text = String::from_utf8_lossy(&all);
                if text.contains(&expected) && text.contains("한글왕복") {
                    passed = true;
                    break;
                }
            }
            Err(_) => {}
        }
    }

    // Cleanup must run regardless of outcome.
    let _ = writer.write_all(b"exit\r\n");
    let _ = writer.flush();
    drop(writer);
    let _ = child.kill();
    drop(pair.master);
    let _ = child.wait();
    let _ = reader_handle.join();

    if !passed {
        let limited: Vec<u8> = all.iter().take(4096).copied().collect();
        if !query_observed {
            panic!(
                "DSR cursor query ESC[6n was not observed in the allotted test timeout; transcript(first 4096 bytes debug-escaped)={:?}",
                limited
            );
        } else {
            panic!(
                "No confirmed real ConPTY output in the allotted test timeout; transcript(first 4096 bytes debug-escaped)={:?}",
                limited
            );
        }
    }

    assert!(
        passed,
        "test should have passed before reaching this assertion"
    );
}
