//! `cargo test -p ide-tools --test unity_debugger -- --ignored --nocapture`
//! Installs Prefabric's Unity debugger (a .NET app) from its GitHub release into a temporary
//! managed-tools directory, then starts it and checks it answers DAP's `initialize`.
use ide_tools::{InstallEvent, ToolStatus, Tools};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

#[test]
#[ignore = "requires network; downloads the Unity debugger (about 6 MB) from GitHub and .NET when missing"]
fn real_unity_debugger_install_from_github() {
    if std::env::var_os("OPENIDEA_TEST_OFFLINE").is_some() {
        eprintln!("SKIP real Unity debugger: OPENIDEA_TEST_OFFLINE is set");
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let tools = Tools::new(&temp.path().join("app-data"));
    let lines = parking_lot::Mutex::new(Vec::new());
    let emit = |e: InstallEvent| {
        if let InstallEvent::Line { text } = e {
            eprintln!("{text}");
            lines.lock().push(text);
        }
    };
    let info = tools.install("unity-debugger", &[], &Default::default(), &emit).unwrap();
    assert_eq!(info.status, ToolStatus::Installed);
    assert!(lines.lock().iter().any(|l| l == "Checksum verified (SHA-256)"), "{:?}", lines.lock());

    let path = info.path.unwrap();
    let mut child = Command::new(AsRef::<std::path::Path>::as_ref(&path))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let body = r#"{"seq":1,"type":"request","command":"initialize","arguments":{"adapterID":"unity"}}"#;
    write!(child.stdin.as_mut().unwrap(), "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut header = String::new();
    out.read_line(&mut header).unwrap();
    let len: usize = header.trim().strip_prefix("Content-Length:").unwrap().trim().parse().unwrap();
    out.read_line(&mut String::new()).unwrap();
    let mut reply = vec![0; len];
    out.read_exact(&mut reply).unwrap();
    let _ = child.kill();
    let _ = child.wait();
    let reply: serde_json::Value = serde_json::from_slice(&reply).unwrap();
    assert_eq!(reply["command"], "initialize");
    assert_eq!(reply["success"], true, "{reply}");
}
