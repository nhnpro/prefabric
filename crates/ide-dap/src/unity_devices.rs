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

fn run(program: &Path, args: &[&str]) -> Option<String> {
    let out = ide_core::process::prepare(&mut Command::new(program)).args(args).stdin(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn on_path(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() };
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(&exe)).find(|p| p.is_file())
}

/// Unity Hub's editors, newest version first (`Hub/Editor/<version>`).
fn hub_editors() -> Vec<PathBuf> {
    let roots: Vec<PathBuf> = if cfg!(target_os = "macos") {
        vec!["/Applications/Unity/Hub/Editor".into()]
    } else if cfg!(windows) {
        vec!["C:\\Program Files\\Unity\\Hub\\Editor".into()]
    } else {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Unity/Hub/Editor")).into_iter().collect()
    };
    let mut dirs: Vec<PathBuf> = roots
        .iter()
        .filter_map(|r| std::fs::read_dir(r).ok())
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    dirs.reverse();
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

/// `iproxy` processes kept alive while the IDE runs, by `udid:port`.
static PROXIES: Mutex<Vec<(String, u16, Child)>> = Mutex::new(Vec::new());

fn free_local_port() -> IdeResult<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| IdeError::unavailable(e.to_string()))?;
    Ok(listener.local_addr().map_err(|e| IdeError::unavailable(e.to_string()))?.port())
}

/// Maps a device target's port to a local one and returns it. Blocking.
pub fn forward(host: &str, port: u16) -> IdeResult<u16> {
    if let Some(serial) = host.strip_prefix(ADB_PREFIX) {
        let adb = find_adb().ok_or_else(|| {
            IdeError::unavailable("adb not found: install the Android SDK or Unity's Android Build Support")
        })?;
        // `tcp:0` lets adb pick the local port and print it.
        let out = run(&adb, &["-s", serial, "forward", "tcp:0", &format!("tcp:{port}")]).ok_or_else(|| {
            IdeError::unavailable(format!("adb forward to {serial} failed (is the device still connected?)"))
        })?;
        return out.trim().parse().map_err(|_| IdeError::unavailable(format!("unexpected adb forward output: {out}")));
    }
    if let Some(udid) = host.strip_prefix(USB_PREFIX) {
        let mut proxies = PROXIES.lock();
        proxies.retain_mut(|(_, _, child)| matches!(child.try_wait(), Ok(None)));
        if let Some((_, local, _)) = proxies.iter().find(|(u, _, _)| u == udid) {
            return Ok(*local);
        }
        let iproxy =
            on_path("iproxy").ok_or_else(|| IdeError::unavailable("iproxy not found: install libimobiledevice"))?;
        let local = free_local_port()?;
        let child = ide_core::process::prepare(&mut Command::new(iproxy))
            .args([&format!("{local}:{port}"), "-u", udid])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| IdeError::unavailable(format!("cannot start iproxy: {e}")))?;
        proxies.push((udid.to_owned(), local, child));
        // Give it a moment to listen before the adapter connects.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            if std::net::TcpListener::bind(("127.0.0.1", local)).is_err() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
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
}
