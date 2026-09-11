#![cfg(windows)]
//! Opt-in Windows process tests. Never silently skipped and counted as Windows PASS on Linux.
use portable_pty::{native_pty_system,CommandBuilder,PtySize};
use std::{io::{Read,Write},time::{Duration,Instant}};
#[test]
#[ignore = "Requires actual Windows ConPTY. Run with --ignored --nocapture on the user's PC"]
fn cmd_real_pty_round_trip(){
    let pair=native_pty_system().openpty(PtySize{cols:120,rows:32,pixel_width:0,pixel_height:0}).unwrap();
    let mut command=CommandBuilder::new("cmd.exe");command.args(["/D","/Q"]);
    let mut child=pair.slave.spawn_command(command).unwrap();drop(pair.slave);
    let mut reader=pair.master.try_clone_reader().unwrap();let mut writer=pair.master.take_writer().unwrap();
    let(tx,rx)=std::sync::mpsc::channel();let reader=std::thread::spawn(move||{let mut b=[0u8;4096];while let Ok(n)=reader.read(&mut b){if n==0{break;}if tx.send(b[..n].to_vec()).is_err(){break;}}});
    let marker=format!("RC_PTY_OK_{}",uuid::Uuid::new_v4());
    // The expected outputs do not appear verbatim in the input, preventing command-echo false positives.
    let expected=format!("{marker}_RESULT");
    writer.write_all(format!("@chcp 65001 >nul\r\n@set \"RC_MARK={marker}\"\r\n@echo %RC_MARK%_RESULT\r\n@set \"RC_KR=한글\"\r\n@echo %RC_KR%왕복\r\n").as_bytes()).unwrap();
    let until=Instant::now()+Duration::from_secs(10);let mut all=Vec::new();let mut passed=false;
    while Instant::now()<until{if let Ok(b)=rx.recv_timeout(Duration::from_millis(250)){all.extend(b);let text=String::from_utf8_lossy(&all);if text.contains(&expected)&&text.contains("한글왕복"){passed=true;break;}}}
    let _=writer.write_all(b"exit\r\n");let _=child.kill();drop(writer);drop(pair.master);let _=child.wait();let _=reader.join();
    assert!(passed,"No confirmed real ConPTY output in the allotted test timeout");
}
