//! The IDE side of the Unity Editor link (`docs/unity-link.md`): finds the Editor through
//! `Library/Prefabric/link.json`, connects on loopback, sends the token handshake, follows
//! `editor/stateChanged` and reconnects after domain reloads. One background thread per project.

use ide_core::{
    Diagnostic, IdeError, IdeEvent, IdeResult, Pos, Range, Severity, SharedSink, UnityEditorState, UnityLinkConnection,
    UnityLinkStatus, UnityLogEntry, UnityPlayMode,
};
use parking_lot::{Condvar, Mutex};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use specta::Type;
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

mod test_run;
#[cfg(test)]
mod tests;

pub use test_run::{
    MARKER as TEST_MARKER, TestRun, TestUpdate, console_text as test_console_text, exit_code as test_exit_code,
};

/// The protocol version this IDE speaks.
pub const PROTOCOL: u32 = 1;
const REFUSED_OLDER_PROTOCOL: i64 = -32004;
/// How often the link file is checked while no Editor is running.
const POLL: Duration = Duration::from_secs(1);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const BACKOFF_START: Duration = Duration::from_millis(500);
const BACKOFF_MAX: Duration = Duration::from_secs(5);
/// A domain reload closes the socket for about a second; the status bar shows "reconnecting"
/// only when it takes longer than this.
const QUIET_RECONNECT: Duration = Duration::from_secs(3);

/// Diagnostics source of the Editor's compile errors.
pub const SOURCE: &str = "unity";

/// What the IDE asks the Editor to do (the toolbar and its actions).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UnityEditorCommand {
    Play,
    Stop,
    /// Toggles pause.
    Pause,
    /// Pauses (a pausepoint): unlike [`Self::Pause`], never resumes.
    PausePlay,
    Step,
    RefreshAssets,
}

impl UnityEditorCommand {
    fn method(self) -> &'static str {
        match self {
            Self::Play => "editor.play",
            Self::Stop => "editor.stop",
            Self::Pause | Self::PausePlay => "editor.pause",
            Self::Step => "editor.step",
            Self::RefreshAssets => "assets.refresh",
        }
    }
}

/// `Library/Prefabric/link.json`, written by the Editor package.
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LinkFile {
    pub protocol: u32,
    pub port: u16,
    pub token: String,
    pub pid: u32,
    #[serde(default)]
    pub unity_version: Option<String>,
}

pub fn link_file(root: &Path) -> PathBuf {
    root.join("Library").join("Prefabric").join("link.json")
}

/// The link file, if an Editor that is still running wrote it.
fn read_link_file(path: &Path) -> Option<LinkFile> {
    let file: LinkFile = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    ide_core::process::is_running(file.pid).then_some(file)
}

type Reply = mpsc::Sender<IdeResult<Value>>;

struct Connection {
    stream: TcpStream,
    next_id: u64,
    pending: HashMap<u64, Reply>,
}

struct Inner {
    root: PathBuf,
    file: PathBuf,
    /// Files that have compile diagnostics from the Editor, to clear on the next compile.
    reported: Mutex<Vec<PathBuf>>,
    sink: SharedSink,
    status: Mutex<UnityLinkStatus>,
    connection: Mutex<Option<Connection>>,
    /// Listeners of the test runs in progress, by the IDE's run id.
    test_runs: Mutex<HashMap<String, test_run::Listener>>,
    stopped: Mutex<bool>,
    wake: Condvar,
}

/// The link of one open Unity project. Dropping it disconnects.
pub struct UnityLink {
    inner: Arc<Inner>,
}

impl UnityLink {
    pub fn start(root: &Path, sink: SharedSink) -> Self {
        let inner = Arc::new(Inner {
            root: root.to_owned(),
            file: link_file(root),
            reported: Mutex::default(),
            sink,
            status: Mutex::default(),
            connection: Mutex::new(None),
            test_runs: Mutex::default(),
            stopped: Mutex::new(false),
            wake: Condvar::new(),
        });
        let worker = inner.clone();
        let spawned = std::thread::Builder::new().name("unity-link".into()).spawn(move || worker.run());
        if let Err(e) = spawned {
            tracing::warn!("unity link: cannot start its thread: {e}");
        }
        Self { inner }
    }

    pub fn status(&self) -> UnityLinkStatus {
        self.inner.status.lock().clone()
    }

    /// Runs a toolbar command in the Editor. Blocking.
    pub fn command(&self, command: UnityEditorCommand) -> IdeResult<()> {
        let params = if command == UnityEditorCommand::PausePlay { json!({"paused": true}) } else { json!({}) };
        self.request(command.method(), params, Duration::from_secs(10)).map(drop)
    }

    /// Selects and pings in the Editor what `path` holds at `line`: the GameObject or component of a
    /// scene or prefab (from the YAML object around the line), otherwise the asset itself. Returns
    /// whether Unity found it; a scene that isn't open gets its asset pinged instead. Blocking.
    pub fn show_in_editor(&self, path: &Path, line: Option<u32>) -> IdeResult<bool> {
        let root = dunce::simplified(&self.inner.root);
        let rel = dunce::simplified(path)
            .strip_prefix(root)
            .map_err(|_| IdeError::invalid("Not in the Unity project"))?
            .to_string_lossy()
            .replace('\\', "/");
        let object = line.filter(|_| is_yaml_asset(path)).and_then(|line| {
            let text = std::fs::read_to_string(path).ok()?;
            object_at(&text, line)
        });
        let timeout = Duration::from_secs(10);
        let found = match object {
            Some(object) if object.class != PREFAB_INSTANCE && !object.stripped => {
                self.request("object.select", json!({"path": rel, "fileId": object.file_id}), timeout)?
            }
            _ => self.request("asset.ping", json!({"path": rel}), timeout)?,
        };
        Ok(found.as_bool().unwrap_or(false))
    }

    /// Sends a request to the Editor and waits for its result. Blocking.
    pub fn request(&self, method: &str, params: Value, timeout: Duration) -> IdeResult<Value> {
        let (tx, rx) = mpsc::channel();
        let id = {
            let mut guard = self.inner.connection.lock();
            let connection = guard.as_mut().ok_or_else(|| IdeError::unavailable("Unity Editor is not connected"))?;
            connection.next_id += 1;
            let id = connection.next_id;
            let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
            writeln!(connection.stream, "{line}").map_err(|e| IdeError::unavailable(format!("Unity link: {e}")))?;
            connection.pending.insert(id, tx);
            id
        };
        rx.recv_timeout(timeout).unwrap_or_else(|_| {
            if let Some(c) = self.inner.connection.lock().as_mut() {
                c.pending.remove(&id);
            }
            Err(IdeError::unavailable(format!("Unity didn't answer {method}")))
        })
    }
}

impl Drop for UnityLink {
    fn drop(&mut self) {
        *self.inner.stopped.lock() = true;
        self.inner.wake.notify_all();
        if let Some(c) = self.inner.connection.lock().as_ref() {
            let _ = c.stream.shutdown(Shutdown::Both);
        }
    }
}

enum Ended {
    /// Connected, then the Editor closed the socket (domain reload, quit).
    Closed,
    Incompatible,
    Failed,
}

impl Inner {
    fn stopped(&self) -> bool {
        *self.stopped.lock()
    }

    /// Sleeps for `d` unless the link is dropped first; returns false once it is.
    fn wait(&self, d: Duration) -> bool {
        let mut stopped = self.stopped.lock();
        if !*stopped {
            self.wake.wait_for(&mut stopped, d);
        }
        !*stopped
    }

    fn set(&self, status: UnityLinkStatus) {
        let mut current = self.status.lock();
        if *current != status {
            *current = status.clone();
            drop(current);
            self.sink.emit(IdeEvent::UnityLink { status });
        }
    }

    fn set_connection(&self, connection: UnityLinkConnection) {
        let mut status = self.status.lock().clone();
        status.connection = connection;
        if connection != UnityLinkConnection::Connected {
            status.editor = None;
        }
        self.set(status);
    }

    fn run(self: Arc<Self>) {
        let mut backoff = BACKOFF_START;
        let mut lost_at: Option<Instant> = None;
        while !self.stopped() {
            let Some(file) = read_link_file(&self.file) else {
                // Right after a reload the Editor rewrites the file: a missed read isn't a quit.
                if lost_at.is_some_and(|t| t.elapsed() < QUIET_RECONNECT) {
                    if !self.wait(Duration::from_millis(250)) {
                        break;
                    }
                    continue;
                }
                self.set(UnityLinkStatus::default());
                self.lose_test_runs();
                self.publish_compile(&[]);
                lost_at = None;
                backoff = BACKOFF_START;
                if !self.wait(POLL) {
                    break;
                }
                continue;
            };
            let connected_before = self.status.lock().connection == UnityLinkConnection::Connected;
            if !connected_before {
                self.set_connection(UnityLinkConnection::Connecting);
            }
            match self.session(&file) {
                Ended::Closed => {
                    backoff = BACKOFF_START;
                    lost_at = Some(Instant::now());
                }
                Ended::Incompatible => {
                    self.set_connection(UnityLinkConnection::Incompatible);
                    self.lose_test_runs();
                    lost_at = None;
                    backoff = BACKOFF_MAX;
                }
                Ended::Failed => {
                    if lost_at.is_none_or(|t| t.elapsed() >= QUIET_RECONNECT) {
                        self.set_connection(UnityLinkConnection::Connecting);
                        backoff = (backoff * 2).min(BACKOFF_MAX);
                    }
                }
            }
            // Right after a reload, retry quickly and keep showing the last state.
            let delay = if lost_at.is_some_and(|t| t.elapsed() < QUIET_RECONNECT) {
                Duration::from_millis(250)
            } else {
                backoff
            };
            if !self.wait(delay) {
                break;
            }
        }
        self.set(UnityLinkStatus::default());
        self.lose_test_runs();
    }

    /// One connection: handshake, then notifications and replies until the socket closes.
    fn session(&self, file: &LinkFile) -> Ended {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, file.port));
        let Ok(stream) = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) else { return Ended::Failed };
        let _ = stream.set_nodelay(true);
        let Ok(read_half) = stream.try_clone() else { return Ended::Failed };
        let mut reader = BufReader::new(read_half);
        let hello = json!({
            "jsonrpc": "2.0", "id": 0, "method": "hello",
            "params": {"token": file.token, "protocol": PROTOCOL, "client": concat!("Prefabric ", env!("CARGO_PKG_VERSION"))},
        });
        if stream.set_read_timeout(Some(HELLO_TIMEOUT)).is_err() || writeln!(&stream, "{hello}").is_err() {
            return Ended::Failed;
        }
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return Ended::Failed;
        }
        let Ok(reply) = serde_json::from_str::<Value>(&line) else { return Ended::Failed };
        if reply.pointer("/error/code").and_then(Value::as_i64) == Some(REFUSED_OLDER_PROTOCOL) {
            return Ended::Incompatible;
        }
        let Some(result) = reply.get("result") else { return Ended::Failed };
        if result.get("protocol").and_then(Value::as_u64).unwrap_or(0) < u64::from(PROTOCOL) {
            return Ended::Incompatible;
        }
        let _ = stream.set_read_timeout(None);
        // The last compile's messages, which a reload or a late connection would otherwise miss.
        if let Some(messages) = result.pointer("/compile/messages").and_then(Value::as_array) {
            self.publish_compile(messages);
        }
        let Ok(writer) = stream.try_clone() else { return Ended::Failed };
        *self.connection.lock() = Some(Connection { stream: writer, next_id: 0, pending: HashMap::new() });
        if self.stopped() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        self.set(UnityLinkStatus {
            connection: UnityLinkConnection::Connected,
            editor: result.get("state").and_then(parse_state),
            unity_version: result
                .get("unityVersion")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| file.unity_version.clone()),
        });

        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => self.handle(&line),
            }
        }
        if let Some(connection) = self.connection.lock().take() {
            for (_, reply) in connection.pending {
                let _ = reply.send(Err(IdeError::unavailable("The Unity Editor closed the link")));
            }
        }
        Ended::Closed
    }

    fn handle(&self, line: &str) {
        let Ok(message) = serde_json::from_str::<Value>(line) else { return };
        if let Some(id) = message.get("id").and_then(Value::as_u64) {
            let reply = self.connection.lock().as_mut().and_then(|c| c.pending.remove(&id));
            if let Some(reply) = reply {
                let result = match message.get("error") {
                    Some(e) => Err(IdeError::unavailable(
                        e.get("message").and_then(Value::as_str).unwrap_or("Unity returned an error").to_owned(),
                    )),
                    None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
                };
                let _ = reply.send(result);
            }
            return;
        }
        match message.get("method").and_then(Value::as_str) {
            Some("editor/stateChanged") => {
                if let Some(state) = message.get("params").and_then(parse_state) {
                    let mut status = self.status.lock().clone();
                    status.editor = Some(state);
                    self.set(status);
                }
            }
            Some("console/logs") => {
                let entries = message.pointer("/params/entries").cloned().unwrap_or(Value::Null);
                // A malformed entry drops only itself.
                let entries: Vec<UnityLogEntry> = match entries {
                    Value::Array(items) => items.into_iter().filter_map(|e| serde_json::from_value(e).ok()).collect(),
                    _ => Vec::new(),
                };
                if !entries.is_empty() {
                    self.sink.emit(IdeEvent::UnityConsole { entries });
                }
            }
            Some("tests/event") => {
                if let Some(params) = message.get("params") {
                    self.test_event(params);
                }
            }
            Some("compile/finished") => {
                let messages = message.pointer("/params/messages").and_then(Value::as_array);
                self.publish_compile(messages.map(Vec::as_slice).unwrap_or_default());
            }
            _ => {}
        }
    }

    /// Replaces the Editor's compile diagnostics with `messages`.
    fn publish_compile(&self, messages: &[Value]) {
        let files = compile_diagnostics(&self.root, messages);
        let mut reported = self.reported.lock();
        for path in reported.drain(..) {
            if !files.contains_key(&path) {
                self.sink.emit(IdeEvent::Diagnostics { path: path.into(), source: SOURCE.into(), items: Vec::new() });
            }
        }
        for (path, items) in files {
            reported.push(path.clone());
            self.sink.emit(IdeEvent::Diagnostics { path: path.into(), source: SOURCE.into(), items });
        }
    }
}

fn parse_state(v: &Value) -> Option<UnityEditorState> {
    let play_mode = match v.get("playMode")?.as_str()? {
        "playing" => UnityPlayMode::Playing,
        "paused" => UnityPlayMode::Paused,
        _ => UnityPlayMode::Edit,
    };
    Some(UnityEditorState {
        play_mode,
        compiling: v.get("compiling").and_then(Value::as_bool).unwrap_or(false),
        updating: v.get("updating").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// The Editor's compiler messages by file. Unity gives project-relative paths, 1-based lines
/// and columns, and messages like `Assets/A.cs(3,10): error CS1002: ; expected`.
fn compile_diagnostics(root: &Path, messages: &[Value]) -> BTreeMap<PathBuf, Vec<Diagnostic>> {
    let mut files: BTreeMap<PathBuf, Vec<Diagnostic>> = BTreeMap::new();
    for m in messages {
        let Some(file) = m.get("file").and_then(Value::as_str).filter(|f| !f.is_empty()) else { continue };
        let severity = match m.get("severity").and_then(Value::as_str) {
            Some("error") => Severity::Error,
            Some("warning") => Severity::Warning,
            _ => continue,
        };
        let number = |key: &str| m.get(key).and_then(Value::as_u64).unwrap_or(1).saturating_sub(1) as u32;
        let at = Pos { line: number("line"), col: number("column") };
        let (code, text) = split_message(m.get("message").and_then(Value::as_str).unwrap_or_default());
        let path = dunce::simplified(&root.join(file)).to_owned();
        files.entry(path).or_default().push(Diagnostic {
            range: Range { start: at, end: at },
            severity,
            message: text.to_owned(),
            source: Some(SOURCE.into()),
            code: code.map(str::to_owned),
            unnecessary: false,
            deprecated: false,
            server: Some(SOURCE.into()),
        });
    }
    files
}

/// `Assets/A.cs(3,10): error CS1002: ; expected` → (`CS1002`, `; expected`).
fn split_message(message: &str) -> (Option<&str>, &str) {
    let rest = message.split_once("): ").map_or(message, |(_, r)| r);
    let rest = rest.strip_prefix("error ").or_else(|| rest.strip_prefix("warning ")).unwrap_or(rest);
    match rest.split_once(": ") {
        Some((code, text)) if !code.is_empty() && code.chars().all(|c| c.is_ascii_alphanumeric()) => (Some(code), text),
        _ => (None, rest),
    }
}

const PREFAB_INSTANCE: u32 = 1001;

fn is_yaml_asset(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e, "unity" | "prefab" | "asset"))
}

/// A serialized object of a Unity YAML file: `--- !u!<class> &<fileID>[ stripped]`.
#[derive(Debug, PartialEq, Eq)]
struct YamlObject {
    class: u32,
    /// As text: file IDs exceed 2^53.
    file_id: String,
    /// A placeholder for an object of a prefab instance.
    stripped: bool,
}

/// The object whose block holds 0-based `line`.
fn object_at(text: &str, line: u32) -> Option<YamlObject> {
    let lines: Vec<&str> = text.lines().take(line as usize + 1).collect();
    lines.into_iter().rev().find_map(|l| {
        let rest = l.strip_prefix("--- !u!")?;
        let (class, rest) = rest.split_once(" &")?;
        let mut words = rest.split_whitespace();
        let file_id = words.next()?;
        if !file_id.trim_start_matches('-').chars().all(|c| c.is_ascii_digit()) || file_id.is_empty() {
            return None;
        }
        Some(YamlObject {
            class: class.parse().ok()?,
            file_id: file_id.to_owned(),
            stripped: words.any(|w| w == "stripped"),
        })
    })
}
