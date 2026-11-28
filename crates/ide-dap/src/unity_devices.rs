//! Unity players on USB devices: Android through `adb forward`, iOS through `iproxy`
//! (libimobiledevice). Rows carry the device in `host` (`adb:<serial>`, `usb:<udid>`) and the
//! player's debugger port on the device; [`forward`] maps it to a local port before attaching.

use crate::attach::{AttachKind, AttachableProcess};
use crate::unity::ADAPTER;
use ide_core::{IdeError, IdeResult};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

pub const ADB_PREFIX: &str = "adb:";
pub const USB_PREFIX: &str = "usb:";
/// Unity's iOS players listen for the debugger on this device port.
const IOS_PLAYER_PORT: u16 = 56000;

/// Timeout for device discovery/forwarding commands (`adb`, `idevice_id`, `idevicename`).
const CMD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

const MAX_CMD_STDOUT: u64 = 4 * 1024 * 1024;

fn run(program: &Path, args: &[&str]) -> Option<String> {
    let mut child = ide_core::process::prepare(&mut Command::new(program))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        let _ = stdout.take(MAX_CMD_STDOUT).read_to_end(&mut buf);
        buf
    });
    let deadline = std::time::Instant::now() + CMD_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                // Do not join `reader`: a grandchild (e.g. an auto-started adb daemon) may still hold stdout open.
                drop(reader);
                return None;
            }
        }
    };
    let bytes = reader.join().ok()?;
    status.success().then(|| String::from_utf8_lossy(&bytes).into_owned())
}

fn on_path(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() };
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(&exe)).find(|p| p.is_file())
}

/// `6000.6.8f1` → `[6000, 6, 8, 1]`: numeric order, so 2022.3.10 sorts after 2022.3.9.
fn version_key(dir: &Path) -> Vec<u64> {
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    name.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).map(|s| s.parse().unwrap_or(0)).collect()
}

fn hub_config_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData").join("Roaming"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"))
    }
}

/// Unity Hub's editors, newest version first (`Hub/Editor/<version>`).
fn hub_editors() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);
    if let Some(ref home) = home
        && let Ok(text) =
            std::fs::read_to_string(hub_config_dir(home).join("UnityHub").join("secondaryInstallPath.json"))
        && let Ok(custom) = serde_json::from_str::<String>(text.trim())
        && !custom.is_empty()
    {
        roots.push(PathBuf::from(custom));
    }
    if cfg!(target_os = "macos") {
        roots.push("/Applications/Unity/Hub/Editor".into());
    } else if cfg!(windows) {
        let program_files =
            std::env::var_os("ProgramFiles").map(PathBuf::from).unwrap_or_else(|| "C:\\Program Files".into());
        roots.push(program_files.join("Unity").join("Hub").join("Editor"));
    } else if let Some(home) = home {
        roots.push(home.join("Unity/Hub/Editor"));
    }
    let mut dirs: Vec<PathBuf> = roots
        .iter()
        .filter_map(|r| std::fs::read_dir(r).ok())
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort_by_key(|p| std::cmp::Reverse(version_key(p)));
    dirs
}

/// The Android SDK that ships with a Unity editor installation.
fn editor_sdk(editor: &Path) -> PathBuf {
    let engines = if cfg!(target_os = "macos") {
        editor.join("PlaybackEngines")
    } else {
        editor.join("Editor/Data/PlaybackEngines")
    };
    engines.join("AndroidPlayer/SDK")
}

/// `adb`: `ANDROID_HOME`/`ANDROID_SDK_ROOT`, then `PATH`, then the SDK bundled with Unity.
pub fn find_adb() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "adb.exe" } else { "adb" };
    let sdk_adb = |sdk: PathBuf| Some(sdk.join("platform-tools").join(exe)).filter(|p| p.is_file());
    ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
        .iter()
        .filter_map(std::env::var_os)
        .find_map(|sdk| sdk_adb(sdk.into()))
        .or_else(|| on_path("adb"))
        .or_else(|| hub_editors().into_iter().find_map(|e| sdk_adb(editor_sdk(&e))))
}

/// `(serial, model)` of the devices `adb devices -l` lists as ready.
pub fn parse_adb_devices(text: &str) -> Vec<(String, Option<String>)> {
    text.lines()
        .skip_while(|l| !l.starts_with("List of devices"))
        .skip(1)
        .filter_map(|l| {
            let mut words = l.split_whitespace();
            let serial = words.next()?;
            (words.next()? == "device").then(|| {
                let model = words.find_map(|w| w.strip_prefix("model:")).map(|m| m.replace('_', " "));
                (serial.to_owned(), model)
            })
        })
        .collect()
}

/// The last `… managed debugger on port 56123` a Unity player logged.
pub fn parse_player_port(logcat: &str) -> Option<u16> {
    logcat.lines().rev().find_map(|l| {
        let at = l.find("managed debugger on port")? + "managed debugger on port".len();
        l[at..].split_whitespace().next()?.trim_end_matches(|c: char| !c.is_ascii_digit()).parse().ok()
    })
}

/// A stable row id for a device target (rows need distinct `pid`s).
fn row_id(serial: &str, port: u16) -> u32 {
    serial.bytes().fold(port as u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32))
}

fn android(adb: &Path) -> Vec<AttachableProcess> {
    let Some(list) = run(adb, &["devices", "-l"]) else { return Vec::new() };
    parse_adb_devices(&list)
        .into_iter()
        .filter_map(|(serial, model)| {
            let log = run(adb, &["-s", &serial, "logcat", "-d", "-v", "raw", "-e", "managed debugger on port"])?;
            let port = parse_player_port(&log)?;
            let device = model.unwrap_or_else(|| serial.clone());
            Some(AttachableProcess {
                pid: row_id(&serial, port),
                name: format!("Android player — {device}"),
                command_line: format!("Unity player on {device} ({serial}) over USB, debugger port {port}"),
                kind: AttachKind::UnityMono,
                adapter: ADAPTER.to_owned(),
                host: Some(format!("{ADB_PREFIX}{serial}")),
                port: Some(port),
            })
        })
        .collect()
}

fn ios() -> Vec<AttachableProcess> {
    let Some(idevice_id) = on_path("idevice_id") else { return Vec::new() };
    let Some(list) = run(&idevice_id, &["-l"]) else { return Vec::new() };
    list.split_whitespace()
        .map(|udid| {
            let name = on_path("idevicename")
                .and_then(|p| run(&p, &["-u", udid]))
                .map(|n| n.trim().to_owned())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| udid.to_owned());
            AttachableProcess {
                pid: row_id(udid, IOS_PLAYER_PORT),
                name: format!("iOS player — {name}"),
                command_line: format!("Unity player on {name} ({udid}) over USB"),
                kind: AttachKind::UnityMono,
                adapter: ADAPTER.to_owned(),
                host: Some(format!("{USB_PREFIX}{udid}")),
                port: Some(IOS_PLAYER_PORT),
            }
        })
        .collect()
}

/// Unity players on USB devices. Blocking (runs `adb`, `idevice_id`).
pub fn discover() -> Vec<AttachableProcess> {
    let mut rows = find_adb().map(|adb| android(&adb)).unwrap_or_default();
    rows.extend(ios());
    rows
}

struct ProxyEntry {
    udid: String,
    device_port: u16,
    local_port: u16,
    child: Child,
}

struct AdbForwardEntry {
    adb: PathBuf,
    serial: String,
    device_port: u16,
    local_port: u16,
}

/// Active `iproxy` processes keyed by `(udid, device_port)` and cleaned up when their session ends.
static PROXIES: Mutex<Vec<ProxyEntry>> = Mutex::new(Vec::new());
/// Active `adb forward` rules keyed by `(serial, device_port)` and removed when their session ends.
static ADB_FORWARDS: Mutex<Vec<AdbForwardEntry>> = Mutex::new(Vec::new());

fn free_local_port() -> IdeResult<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| IdeError::unavailable(e.to_string()))?;
    Ok(listener.local_addr().map_err(|e| IdeError::unavailable(e.to_string()))?.port())
}

/// Detached port-forwarding handles removed synchronously from the global caches so a quick
/// re-attach cannot reuse a dying local port while blocking teardown runs off-thread.
pub(crate) struct DetachedForwards {
    proxies: Vec<ProxyEntry>,
    adb_forwards: Vec<AdbForwardEntry>,
}

impl DetachedForwards {
    pub(crate) fn is_empty(&self) -> bool {
        self.proxies.is_empty() && self.adb_forwards.is_empty()
    }

    pub(crate) fn cleanup(self) {
        for mut entry in self.proxies {
            let _ = entry.child.kill();
            let _ = entry.child.wait();
        }
        for entry in self.adb_forwards {
            let _ =
                run(&entry.adb, &["-s", &entry.serial, "forward", "--remove", &format!("tcp:{}", entry.local_port)]);
        }
    }
}

/// Synchronously detaches any `iproxy` process or `adb forward` rule bound to `local_port`.
pub(crate) fn detach_local_port(local_port: u16) -> DetachedForwards {
    let mut proxies_out = Vec::new();
    {
        let mut proxies = PROXIES.lock();
        let mut i = 0;
        while i < proxies.len() {
            if proxies[i].local_port == local_port {
                proxies_out.push(proxies.swap_remove(i));
            } else {
                i += 1;
            }
        }
    }
    let mut adb_out = Vec::new();
    {
        let mut forwards = ADB_FORWARDS.lock();
        let mut i = 0;
        while i < forwards.len() {
            if forwards[i].local_port == local_port {
                adb_out.push(forwards.swap_remove(i));
            } else {
                i += 1;
            }
        }
    }
    DetachedForwards { proxies: proxies_out, adb_forwards: adb_out }
}

/// Releases any `iproxy` process or `adb forward` rule bound to `local_port`. Blocking.
#[allow(dead_code)]
pub fn release_local_port(local_port: u16) {
    detach_local_port(local_port).cleanup();
}

/// Maps a device target's port to a local one and returns it. Blocking.
pub fn forward(host: &str, port: u16) -> IdeResult<u16> {
    if let Some(serial) = host.strip_prefix(ADB_PREFIX) {
        if let Some(local) =
            ADB_FORWARDS.lock().iter().find(|e| e.serial == serial && e.device_port == port).map(|e| e.local_port)
        {
            return Ok(local);
        }
        let adb = find_adb().ok_or_else(|| {
            IdeError::unavailable("adb not found: install the Android SDK or Unity's Android Build Support")
        })?;
        // `tcp:0` lets adb pick the local port and print it.
        let out = run(&adb, &["-s", serial, "forward", "tcp:0", &format!("tcp:{port}")]).ok_or_else(|| {
            IdeError::unavailable(format!("adb forward to {serial} failed (is the device still connected?)"))
        })?;
        let local: u16 =
            out.trim().parse().map_err(|_| IdeError::unavailable(format!("unexpected adb forward output: {out}")))?;
        let mut forwards = ADB_FORWARDS.lock();
        if let Some(existing) = forwards.iter().find(|e| e.serial == serial && e.device_port == port) {
            let existing_port = existing.local_port;
            drop(forwards);
            let _ = run(&adb, &["-s", serial, "forward", "--remove", &format!("tcp:{local}")]);
            return Ok(existing_port);
        }
        forwards.push(AdbForwardEntry { adb, serial: serial.to_owned(), device_port: port, local_port: local });
        return Ok(local);
    }
    if let Some(udid) = host.strip_prefix(USB_PREFIX) {
        {
            let mut proxies = PROXIES.lock();
            proxies.retain_mut(|entry| matches!(entry.child.try_wait(), Ok(None)));
            if let Some(entry) = proxies.iter().find(|e| e.udid == udid && e.device_port == port) {
                return Ok(entry.local_port);
            }
        }
        let iproxy =
            on_path("iproxy").ok_or_else(|| IdeError::unavailable("iproxy not found: install libimobiledevice"))?;
        let local = free_local_port()?;
        let mut child = ide_core::process::prepare(&mut Command::new(iproxy))
            .args([&format!("{local}:{port}"), "-u", udid])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| IdeError::unavailable(format!("cannot start iproxy: {e}")))?;
        // Give iproxy a brief startup window outside the global mutex without binding `local`
        // (which would race with iproxy's own bind or false-fail on SO_REUSEADDR / IPv6).
        for _ in 0..8 {
            if let Ok(Some(status)) = child.try_wait() {
                return Err(IdeError::unavailable(format!("iproxy exited early ({status})")));
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(IdeError::unavailable(format!("iproxy exited early ({status})")));
        }
        let mut proxies = PROXIES.lock();
        proxies.retain_mut(|entry| matches!(entry.child.try_wait(), Ok(None)));
        if let Some(existing) = proxies.iter().find(|e| e.udid == udid && e.device_port == port) {
            let existing_port = existing.local_port;
            drop(proxies);
            let _ = child.kill();
            let _ = child.wait();
            return Ok(existing_port);
        }
        proxies.push(ProxyEntry { udid: udid.to_owned(), device_port: port, local_port: local, child });
        return Ok(local);
    }
    Err(IdeError::invalid(format!("not a device target: {host}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_adb_device_lists() {
        let text = "* daemon started successfully\nList of devices attached\n\
                    R58M123ABC             device usb:1-1 product:beyond1 model:SM_G973F device:beyond1 transport_id:3\n\
                    emulator-5554          offline\n\
                    0123456789ABCDEF       unauthorized usb:1-2 transport_id:4\n\
                    192.168.1.7:5555       device product:x model:Pixel_7 device:panther\n\n";
        assert_eq!(
            parse_adb_devices(text),
            vec![
                ("R58M123ABC".to_owned(), Some("SM G973F".to_owned())),
                ("192.168.1.7:5555".to_owned(), Some("Pixel 7".to_owned())),
            ]
        );
        assert!(parse_adb_devices("List of devices attached\n\n").is_empty());
    }

    #[test]
    fn reads_the_players_debugger_port_from_logcat() {
        let log = "Starting managed debugger on port 56111\nother\nStarting managed debugger on port 56742.\n";
        assert_eq!(parse_player_port(log), Some(56742));
        assert_eq!(parse_player_port("nothing here"), None);
    }

    #[test]
    fn device_rows_have_distinct_ids() {
        assert_ne!(row_id("A", 56000), row_id("B", 56000));
        assert_ne!(row_id("A", 56000), row_id("A", 56001));
    }

    /// `cargo test -p ide-dap --lib unity_devices::tests::real -- --ignored --nocapture`, with a
    /// device attached that runs a development build with script debugging.
    #[test]
    #[ignore]
    fn real_device_discovery() {
        eprintln!("adb: {:?}", find_adb());
        for row in discover() {
            eprintln!(
                "{} {:?} {:?} -> local {:?}",
                row.name,
                row.host,
                row.port,
                forward(row.host.as_deref().unwrap(), row.port.unwrap())
            );
        }
    }

    #[test]
    fn rejects_non_device_targets() {
        assert!(forward("127.0.0.1", 56000).is_err());
    }

    #[test]
    fn sorts_unity_editor_versions_numerically() {
        assert!(version_key(Path::new("2022.3.10f1")) > version_key(Path::new("2022.3.9f1")));
        assert!(version_key(Path::new("6000.0.10f1")) > version_key(Path::new("6000.0.2f1")));
        assert!(version_key(Path::new("6000.0.2f1")) > version_key(Path::new("2023.2.20f1")));
    }
}
