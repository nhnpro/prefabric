use super::*;
use ide_core::EventSink;
use std::net::TcpListener;

#[derive(Default)]
struct Events(Mutex<Vec<UnityLinkStatus>>);

impl EventSink for Events {
    fn emit(&self, event: IdeEvent) {
        if let IdeEvent::UnityLink { status } = event {
            self.0.lock().push(status);
        }
    }
}

const TOKEN: &str = "0123456789abcdef0123456789abcdef";

/// A fake Editor: listens on loopback and writes the link file with this process's pid.
fn editor(root: &Path) -> TcpListener {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let file = link_file(root);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let json = json!({
        "protocol": 1, "port": listener.local_addr().unwrap().port(), "token": TOKEN,
        "pid": std::process::id(), "unityVersion": "6000.6.8f1",
    });
    std::fs::write(file, json.to_string()).unwrap();
    listener
}

fn read_message(reader: &mut BufReader<TcpStream>) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

/// Accepts one client, checks its hello and answers it.
fn accept(listener: &TcpListener, protocol: u32) -> (TcpStream, BufReader<TcpStream>) {
    let (stream, _) = listener.accept().unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let hello = read_message(&mut reader);
    assert_eq!(hello["method"], "hello");
    assert_eq!(hello["params"]["token"], TOKEN);
    assert_eq!(hello["params"]["protocol"], PROTOCOL);
    let reply = json!({"jsonrpc": "2.0", "id": hello["id"], "result": {
        "protocol": protocol, "unityVersion": "6000.6.8f1", "projectPath": "/p",
        "state": {"playMode": "edit", "compiling": false, "updating": false},
    }});
    writeln!(&stream, "{reply}").unwrap();
    (stream, reader)
}

fn wait_for(link: &UnityLink, what: impl Fn(&UnityLinkStatus) -> bool) -> UnityLinkStatus {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let status = link.status();
        if what(&status) {
            return status;
        }
        assert!(Instant::now() < until, "timed out; last status {status:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn edit_state() -> UnityEditorState {
    UnityEditorState { play_mode: UnityPlayMode::Edit, compiling: false, updating: false }
}

#[test]
fn connects_follows_state_and_answers_requests() {
    let dir = tempfile::tempdir().unwrap();
    let listener = editor(dir.path());
    let events = Arc::new(Events::default());
    let link = UnityLink::start(dir.path(), events.clone());

    let (stream, mut reader) = accept(&listener, 1);
    let status = wait_for(&link, |s| s.connection == UnityLinkConnection::Connected);
    assert_eq!(status.editor, Some(edit_state()));
    assert_eq!(status.unity_version.as_deref(), Some("6000.6.8f1"));

    let note = json!({"jsonrpc": "2.0", "method": "editor/stateChanged",
        "params": {"playMode": "paused", "compiling": true, "updating": false}});
    writeln!(&stream, "{note}").unwrap();
    let status = wait_for(&link, |s| s.editor.as_ref().is_some_and(|e| e.compiling));
    assert_eq!(status.editor.unwrap().play_mode, UnityPlayMode::Paused);

    let editor_side = std::thread::spawn(move || {
        let request = read_message(&mut reader);
        assert_eq!(request["method"], "editor.play");
        let reply = json!({"jsonrpc": "2.0", "id": request["id"], "result": {"playMode": "playing"}});
        writeln!(&stream, "{reply}").unwrap();
        (stream, reader)
    });
    let result = link.request("editor.play", json!({}), Duration::from_secs(5)).unwrap();
    assert_eq!(result["playMode"], "playing");
    let (_stream, _reader) = editor_side.join().unwrap();

    let seen = events.0.lock().clone();
    assert_eq!(seen.first().map(|s| s.connection), Some(UnityLinkConnection::Connecting));
    assert!(seen.iter().any(|s| s.connection == UnityLinkConnection::Connected));
}

#[test]
fn reconnects_after_a_domain_reload_without_flickering() {
    let dir = tempfile::tempdir().unwrap();
    let listener = editor(dir.path());
    let events = Arc::new(Events::default());
    let link = UnityLink::start(dir.path(), events.clone());
    let (stream, reader) = accept(&listener, 1);
    wait_for(&link, |s| s.connection == UnityLinkConnection::Connected);
    let before = events.0.lock().len();

    // The reload closes the socket; the Editor listens again on the same port.
    drop((stream, reader));
    let (_stream, _reader) = accept(&listener, 1);
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(link.status().connection, UnityLinkConnection::Connected);
    let after = events.0.lock()[before..].to_vec();
    assert!(after.iter().all(|s| s.connection == UnityLinkConnection::Connected), "{after:?}");
}

#[test]
fn disconnects_when_the_editor_quits() {
    let dir = tempfile::tempdir().unwrap();
    let listener = editor(dir.path());
    let link = UnityLink::start(dir.path(), Arc::new(Events::default()));
    let (stream, reader) = accept(&listener, 1);
    wait_for(&link, |s| s.connection == UnityLinkConnection::Connected);

    std::fs::remove_file(link_file(dir.path())).unwrap();
    drop((stream, reader, listener));
    let status = wait_for(&link, |s| s.connection == UnityLinkConnection::Disconnected);
    assert_eq!(status, UnityLinkStatus::default());
    assert!(link.request("editor.play", json!({}), Duration::from_secs(1)).is_err());
}

#[test]
fn ignores_the_file_of_an_editor_that_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let file = link_file(dir.path());
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    // Far above any real pid.
    std::fs::write(&file, r#"{"protocol":1,"port":1,"token":"x","pid":4000000000}"#).unwrap();
    assert!(read_link_file(&file).is_none());
    std::fs::write(&file, format!(r#"{{"protocol":1,"port":1,"token":"x","pid":{}}}"#, std::process::id())).unwrap();
    assert_eq!(read_link_file(&file).map(|f| f.port), Some(1));
}

#[test]
fn an_older_package_is_incompatible() {
    let dir = tempfile::tempdir().unwrap();
    let listener = editor(dir.path());
    let link = UnityLink::start(dir.path(), Arc::new(Events::default()));
    let (_stream, _reader) = accept(&listener, 0);
    wait_for(&link, |s| s.connection == UnityLinkConnection::Incompatible);
}

#[test]
fn a_wrong_token_is_never_reported_as_connected() {
    let dir = tempfile::tempdir().unwrap();
    let listener = editor(dir.path());
    let link = UnityLink::start(dir.path(), Arc::new(Events::default()));
    let (stream, _) = listener.accept().unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let hello = read_message(&mut reader);
    let reply = json!({"jsonrpc": "2.0", "id": hello["id"], "error": {"code": -32001, "message": "bad token"}});
    writeln!(&stream, "{reply}").unwrap();
    drop((stream, reader));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(link.status().connection, UnityLinkConnection::Connecting);
}

#[test]
fn splits_compiler_messages() {
    assert_eq!(split_message("Assets/A.cs(3,10): error CS1002: ; expected"), (Some("CS1002"), "; expected"));
    assert_eq!(
        split_message("Assets/A.cs(1,1): warning CS0414: The field 'x' is assigned but never used"),
        (Some("CS0414"), "The field 'x' is assigned but never used")
    );
    assert_eq!(split_message("plain text: here"), (None, "plain text: here"));
}

#[test]
fn compile_messages_become_diagnostics_and_are_cleared() {
    let dir = tempfile::tempdir().unwrap();
    let listener = editor(dir.path());
    let events = Arc::new(DiagnosticEvents::default());
    let link = UnityLink::start(dir.path(), events.clone());
    let (stream, _reader) = accept(&listener, 1);
    wait_for(&link, |s| s.connection == UnityLinkConnection::Connected);

    let note = json!({"jsonrpc": "2.0", "method": "compile/finished", "params": {"messages": [
        {"file": "Assets/A.cs", "line": 3, "column": 10, "severity": "error", "message": "Assets/A.cs(3,10): error CS1002: ; expected"},
        {"file": "Assets/A.cs", "line": 5, "column": 1, "severity": "warning", "message": "Assets/A.cs(5,1): warning CS0168: unused"},
    ]}});
    writeln!(&stream, "{note}").unwrap();
    let a = dunce::simplified(&dir.path().join("Assets/A.cs")).to_owned();
    let until = Instant::now() + Duration::from_secs(5);
    while events.0.lock().is_empty() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(20));
    }
    {
        let seen = events.0.lock();
        let (path, items) = &seen[0];
        assert_eq!(path, &a);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].range.start, Pos { line: 2, col: 9 });
        assert_eq!(items[0].code.as_deref(), Some("CS1002"));
        assert_eq!(items[0].message, "; expected");
        assert_eq!(items[1].severity, Severity::Warning);
    }

    let fixed = json!({"jsonrpc": "2.0", "method": "compile/finished", "params": {"messages": []}});
    writeln!(&stream, "{fixed}").unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while events.0.lock().len() < 2 {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(20));
    }
    let seen = events.0.lock();
    assert_eq!(seen[1], (a, Vec::new()));
}

#[derive(Default)]
struct DiagnosticEvents(Mutex<Vec<(PathBuf, Vec<Diagnostic>)>>);

impl EventSink for DiagnosticEvents {
    fn emit(&self, event: IdeEvent) {
        if let IdeEvent::Diagnostics { path, source, items } = event {
            assert_eq!(source, SOURCE);
            self.0.lock().push((PathBuf::from(path), items));
        }
    }
}

#[test]
fn console_logs_are_forwarded() {
    #[derive(Default)]
    struct Logs(Mutex<Vec<UnityLogEntry>>);
    impl EventSink for Logs {
        fn emit(&self, event: IdeEvent) {
            if let IdeEvent::UnityConsole { entries } = event {
                self.0.lock().extend(entries);
            }
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let listener = editor(dir.path());
    let logs = Arc::new(Logs::default());
    let link = UnityLink::start(dir.path(), logs.clone());
    let (stream, _reader) = accept(&listener, 1);
    wait_for(&link, |s| s.connection == UnityLinkConnection::Connected);

    let note = json!({"jsonrpc": "2.0", "method": "console/logs", "params": {"entries": [
        {"kind": "warning", "message": "careful", "stackTrace": "A:B () (at Assets/A.cs:3)\n", "time": "12:00:00.000"},
        {"kind": "bogus", "message": "dropped"},
        {"kind": "exception", "message": "NullReferenceException"},
    ]}});
    writeln!(&stream, "{note}").unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while logs.0.lock().len() < 2 {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(20));
    }
    let seen = logs.0.lock();
    assert_eq!(seen[0].kind, ide_core::UnityLogKind::Warning);
    assert_eq!(seen[0].stack_trace, "A:B () (at Assets/A.cs:3)\n");
    assert_eq!(seen[1].kind, ide_core::UnityLogKind::Exception);
    assert_eq!(seen[1].stack_trace, "");
}

#[test]
fn finds_the_yaml_object_around_a_line() {
    let text = "%YAML 1.1\n--- !u!1 &100\nGameObject:\n  m_Name: A\n--- !u!114 &-7493120387424395000\nMonoBehaviour:\n  m_Enabled: 1\n--- !u!4 &300 stripped\nTransform:\n";
    assert_eq!(object_at(text, 0), None);
    assert_eq!(object_at(text, 3), Some(YamlObject { class: 1, file_id: "100".into(), stripped: false }));
    assert_eq!(
        object_at(text, 6),
        Some(YamlObject { class: 114, file_id: "-7493120387424395000".into(), stripped: false })
    );
    assert_eq!(object_at(text, 8), Some(YamlObject { class: 4, file_id: "300".into(), stripped: true }));
}

#[test]
fn shows_objects_and_assets_in_the_editor() {
    let dir = tempfile::tempdir().unwrap();
    let listener = editor(dir.path());
    let link = UnityLink::start(dir.path(), Arc::new(Events::default()));
    let (stream, mut reader) = accept(&listener, 1);
    wait_for(&link, |s| s.connection == UnityLinkConnection::Connected);
    let scene = dir.path().join("Assets/Main.unity");
    std::fs::create_dir_all(scene.parent().unwrap()).unwrap();
    std::fs::write(&scene, "%YAML 1.1\n--- !u!1 &42\nGameObject:\n  m_Name: Player\n").unwrap();

    let editor_side = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for _ in 0..2 {
            let request = read_message(&mut reader);
            seen.push((request["method"].as_str().unwrap().to_owned(), request["params"].clone()));
            writeln!(&stream, "{}", json!({"jsonrpc": "2.0", "id": request["id"], "result": true})).unwrap();
        }
        seen
    });
    assert!(link.show_in_editor(&scene, Some(3)).unwrap());
    assert!(link.show_in_editor(&dir.path().join("Assets/Main.unity"), None).unwrap());
    let seen = editor_side.join().unwrap();
    assert_eq!(seen[0], ("object.select".into(), json!({"path": "Assets/Main.unity", "fileId": "42"})));
    assert_eq!(seen[1], ("asset.ping".into(), json!({"path": "Assets/Main.unity"})));
    assert!(link.show_in_editor(Path::new("/elsewhere/A.cs"), None).is_err());
}

fn collect_updates(link: &UnityLink, id: &str, run: &TestRun) -> (Arc<Mutex<Vec<String>>>, IdeResult<()>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let out = seen.clone();
    let result = link.run_tests(id, run, move |update| {
        let (stdout, stderr) = test_console_text(&update);
        out.lock().push(format!("{stdout}{stderr}{:?}", test_exit_code(&update)));
    });
    (seen, result)
}

fn wait_len(seen: &Mutex<Vec<String>>, n: usize) {
    let until = Instant::now() + Duration::from_secs(10);
    while seen.lock().len() < n {
        assert!(Instant::now() < until, "got {:?}", seen.lock());
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn runs_tests_and_reports_their_events() {
    let dir = tempfile::tempdir().unwrap();
    let listener = editor(dir.path());
    let link = Arc::new(UnityLink::start(dir.path(), Arc::new(Events::default())));
    let (stream, mut reader) = accept(&listener, 1);
    wait_for(&link, |s| s.connection == UnityLinkConnection::Connected);

    let run = TestRun { mode: "editMode".into(), assembly: Some("Game.Tests".into()), filter: vec!["Game.T.A".into()] };
    let runner = {
        let link = link.clone();
        let run = run.clone();
        std::thread::spawn(move || collect_updates(&link, "r1", &run))
    };
    let request = read_message(&mut reader);
    assert_eq!(request["method"], "tests.run");
    assert_eq!(
        request["params"],
        json!({"runId": "r1", "mode": "editMode", "assembly": "Game.Tests", "filter": ["Game.T.A"]})
    );
    // Events may come before the answer: the listener is registered first.
    let event = |params: Value| json!({"jsonrpc": "2.0", "method": "tests/event", "params": params});
    writeln!(&stream, "{}", event(json!({"runId": "r1", "event": "runStarted"}))).unwrap();
    writeln!(&stream, "{}", event(json!({"runId": "other", "event": "runStarted"}))).unwrap();
    writeln!(&stream, "{}", json!({"jsonrpc": "2.0", "id": request["id"], "result": null})).unwrap();
    let (seen, result) = runner.join().unwrap();
    result.unwrap();
    writeln!(&stream, "{}", event(json!({"runId": "r1", "event": "testFinished", "fullName": "Game.T.A",
        "status": "failed", "durationMs": 3.4, "message": "Expected 2", "stackTrace": "at Game.T.A () in /p/T.cs:9\n", "output": "log"}))).unwrap();
    writeln!(
        &stream,
        "{}",
        event(
            json!({"runId": "r1", "event": "runFinished", "passed": 0, "failed": 1, "skipped": 0, "durationMs": 120})
        )
    )
    .unwrap();
    wait_len(&seen, 3);
    let seen = seen.lock().clone();
    assert!(seen[0].starts_with(&format!("{TEST_MARKER}{{")) && seen[0].ends_with("None"), "{}", seen[0]);
    assert!(
        seen[1].contains("\nlog\nFailed Game.T.A (3 ms)\nExpected 2\nat Game.T.A () in /p/T.cs:9\nNone"),
        "{}",
        seen[1]
    );
    assert!(seen[2].ends_with("\n0 passed, 1 failed, 0 skipped (0.1 s)\nSome(Some(1))"), "{}", seen[2]);
    // Finished: later events of the run are ignored.
    assert!(link.inner.test_runs.lock().is_empty());
    drop(stream);
}

#[test]
fn a_test_run_ends_when_the_editor_goes_away() {
    let dir = tempfile::tempdir().unwrap();
    let listener = editor(dir.path());
    let link = Arc::new(UnityLink::start(dir.path(), Arc::new(Events::default())));
    let (stream, mut reader) = accept(&listener, 1);
    wait_for(&link, |s| s.connection == UnityLinkConnection::Connected);
    let runner = {
        let link = link.clone();
        std::thread::spawn(move || collect_updates(&link, "r2", &TestRun::default()))
    };
    let request = read_message(&mut reader);
    writeln!(&stream, "{}", json!({"jsonrpc": "2.0", "id": request["id"], "result": null})).unwrap();
    let (seen, result) = runner.join().unwrap();
    result.unwrap();

    std::fs::remove_file(link_file(dir.path())).unwrap();
    drop((stream, reader, listener));
    wait_len(&seen, 1);
    assert_eq!(seen.lock()[0], "The Unity Editor closed before the tests finished.\nSome(None)");
}

#[test]
fn a_package_without_the_test_runner_says_what_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let listener = editor(dir.path());
    let link = Arc::new(UnityLink::start(dir.path(), Arc::new(Events::default())));
    let (stream, mut reader) = accept(&listener, 1);
    wait_for(&link, |s| s.connection == UnityLinkConnection::Connected);
    let runner = {
        let link = link.clone();
        std::thread::spawn(move || collect_updates(&link, "r3", &TestRun::default()))
    };
    let request = read_message(&mut reader);
    let reply = json!({"jsonrpc": "2.0", "id": request["id"], "error": {"code": -32601, "message": "unknown method tests.run"}});
    writeln!(&stream, "{reply}").unwrap();
    let (_, result) = runner.join().unwrap();
    assert!(result.unwrap_err().to_string().contains("Test Framework package"));
    assert!(link.inner.test_runs.lock().is_empty());
}

#[test]
fn a_pausepoint_pauses_without_toggling() {
    let dir = tempfile::tempdir().unwrap();
    let listener = editor(dir.path());
    let link = Arc::new(UnityLink::start(dir.path(), Arc::new(Events::default())));
    let (stream, mut reader) = accept(&listener, 1);
    wait_for(&link, |s| s.connection == UnityLinkConnection::Connected);
    for (command, params) in
        [(UnityEditorCommand::PausePlay, json!({"paused": true})), (UnityEditorCommand::Pause, json!({}))]
    {
        let sender = {
            let link = link.clone();
            std::thread::spawn(move || link.command(command))
        };
        let request = read_message(&mut reader);
        assert_eq!((request["method"].as_str(), &request["params"]), (Some("editor.pause"), &params));
        writeln!(&stream, "{}", json!({"jsonrpc": "2.0", "id": request["id"], "result": null})).unwrap();
        sender.join().unwrap().unwrap();
    }
}
