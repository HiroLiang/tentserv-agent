use std::process::{Command, Stdio};

use crate::features::server::ports::ServerProcessProbe;

use super::StdServerProcessProbe;

#[test]
fn windows_server_probe_detects_current_process() {
    assert!(StdServerProcessProbe
        .is_process_running(std::process::id())
        .expect("current process probe"));
}

#[test]
fn windows_server_probe_detects_exited_child() {
    let mut child = Command::new("cmd.exe")
        .args(["/C", "exit", "0"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn short-lived child");
    let pid = child.id();
    child.wait().expect("wait for child exit");

    assert!(!StdServerProcessProbe
        .is_process_running(pid)
        .expect("exited process probe"));
}
