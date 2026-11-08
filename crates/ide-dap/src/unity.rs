//! Unity Mono debugging targets: running Editors (debugger port `56000 + pid % 1000`) and
//! development players, which announce themselves over UDP multicast `225.0.0.222`.

use crate::attach::{AttachKind, AttachableProcess, RawProcess};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, Instant};

/// The Mono debug adapter (`nhnpro/prefabric-unity-debugger`).
pub const ADAPTER: &str = "unity-debugger";

const MULTICAST_GROUP: Ipv4Addr = Ipv4Addr::new(225, 0, 0, 222);
/// Ports Unity players send their announcements to.
const MULTICAST_PORTS: [u16; 4] = [54997, 34997, 57997, 58997];

/// The Mono debugger port of a Unity Editor or of a player with this GUID.
fn debugger_port(id: u32) -> u16 {
    56000 + (id % 1000) as u16
}

/// `-projectPath <dir>` (any case) from an Editor command line.
fn project_path(args: &str) -> Option<&str> {
    let lower = args.to_ascii_lowercase();
    let at = lower.find("-projectpath ")? + "-projectpath ".len();
    let rest = args[at..].trim_start();
    // The path may contain spaces; it runs up to the next ` -option`.
    let end = rest.find(" -").unwrap_or(rest.len());
    Some(rest[..end].trim_end()).filter(|p| !p.is_empty())
}

/// A Unity Editor (not an asset import worker or another helper).
pub fn editor(p: &RawProcess, name: &str) -> Option<AttachableProcess> {
    let base = name.strip_suffix(".exe").unwrap_or(name);
    if base != "Unity" || p.args.contains("AssetImportWorker") {
        return None;
    }
    let project = project_path(&p.args);
    let label = match project.map(|d| d.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or(d)) {
        Some(dir) => format!("Unity Editor — {dir}"),
        None => "Unity Editor".to_owned(),
    };
    Some(AttachableProcess {
        pid: p.pid,
        name: label,
        command_line: p.args.clone(),
        kind: AttachKind::UnityMono,
        adapter: ADAPTER.to_owned(),
        host: None,
        port: Some(debugger_port(p.pid)),
    })
}

/// One player announcement, e.g. `[IP] 10.0.0.5 [Port] 55000 [Flags] 3 [Guid] 2049 [EditorId] 1
/// [Version] 1048832 [Id] OSXPlayer(1,mac):56049 [Debug] 1 [PackageName] OSXPlayer [ProjectName] Game`.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerAnnouncement {
    pub ip: String,
    pub guid: u32,
    pub id: String,
    pub debugger_port: u16,
    pub debug: bool,
    pub project: Option<String>,
}

fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let tag = format!("[{key}] ");
    let at = text.find(&tag)? + tag.len();
    let rest = &text[at..];
    let end = rest.find(" [").unwrap_or(rest.len());
    Some(rest[..end].trim())
}

pub fn parse_announcement(text: &str) -> Option<PlayerAnnouncement> {
    let text = text.trim_end_matches('\0').trim();
    let ip = field(text, "IP")?.to_owned();
    let guid: u32 = field(text, "Guid")?.parse().ok()?;
    let raw_id = field(text, "Id")?;
    // `Id` may carry an explicit debugger port after a colon.
    let (id, port) = match raw_id.rsplit_once(':') {
        Some((id, port)) if port.chars().all(|c| c.is_ascii_digit()) => (id, port.parse().ok()),
        _ => (raw_id, None),
    };
    Some(PlayerAnnouncement {
        ip,
        guid,
        id: id.to_owned(),
        debugger_port: port.unwrap_or_else(|| debugger_port(guid)),
        debug: field(text, "Debug") == Some("1"),
        project: field(text, "ProjectName").filter(|p| !p.is_empty()).map(str::to_owned),
    })
}

impl PlayerAnnouncement {
    /// As a row of the Attach picker; `pid` holds the player's GUID (there is no local process).
    pub fn into_attachable(self) -> AttachableProcess {
        let name = match &self.project {
            Some(project) => format!("{project} — {}", self.id),
            None => self.id.clone(),
        };
        AttachableProcess {
            pid: self.guid,
            name,
            command_line: format!("Unity player {} at {}", self.id, self.ip),
            kind: AttachKind::UnityMono,
            adapter: ADAPTER.to_owned(),
            host: Some(self.ip),
            port: Some(self.debugger_port),
        }
    }
}

fn multicast_socket(port: u16) -> std::io::Result<std::net::UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into())?;
    socket.join_multicast_v4(&MULTICAST_GROUP, &Ipv4Addr::UNSPECIFIED)?;
    Ok(socket.into())
}

/// Development players with script debugging that announce themselves within `wait`.
/// Blocking. Players announce about once a second.
pub fn discover_players(wait: Duration) -> Vec<AttachableProcess> {
    let deadline = Instant::now() + wait;
    let listeners: Vec<_> = MULTICAST_PORTS
        .iter()
        .filter_map(|&port| match multicast_socket(port) {
            Ok(socket) => Some(socket),
            Err(e) => {
                tracing::debug!(port, error = %e, "cannot listen for Unity players");
                None
            }
        })
        .map(|socket| {
            std::thread::spawn(move || {
                let mut found = Vec::new();
                let mut buf = [0u8; 1024];
                loop {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() || socket.set_read_timeout(Some(left)).is_err() {
                        break;
                    }
                    match socket.recv(&mut buf) {
                        Ok(n) => found.extend(parse_announcement(&String::from_utf8_lossy(&buf[..n]))),
                        Err(_) => break,
                    }
                }
                found
            })
        })
        .collect();
    let mut players: Vec<PlayerAnnouncement> = Vec::new();
    for found in listeners.into_iter().filter_map(|t| t.join().ok()) {
        for p in found.into_iter().filter(|p| p.debug) {
            if !players.iter().any(|q| q.guid == p.guid && q.ip == p.ip) {
                players.push(p);
            }
        }
    }
    players.sort_by(|a, b| a.id.cmp(&b.id));
    players.into_iter().map(PlayerAnnouncement::into_attachable).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(exe: &str, args: &str) -> RawProcess {
        RawProcess { pid: 12345, exe: exe.into(), args: args.into() }
    }

    #[test]
    fn finds_editors_but_not_workers() {
        let exe = "/Applications/Unity/Hub/Editor/6000.6.8f1/Unity.app/Contents/MacOS/Unity";
        let e = editor(&raw(exe, &format!("{exe} -projectpath /Users/me/My Game -useHub -hubIPC")), "Unity").unwrap();
        assert_eq!(e.name, "Unity Editor — My Game");
        assert_eq!(e.port, Some(56345));
        assert_eq!(e.kind, AttachKind::UnityMono);
        let w = raw(exe, &format!("{exe} -batchMode -noUpm -name AssetImportWorker0 -projectPath /p"));
        assert!(editor(&w, "Unity").is_none());
        assert!(editor(&raw("C:\\Unity\\Editor\\Unity.exe", "Unity.exe"), "Unity.exe").is_some());
        assert!(editor(&raw("/x/Unity Hub", "Unity Hub"), "Unity Hub").is_none());
        assert_eq!(editor(&raw(exe, exe), "Unity").unwrap().name, "Unity Editor");
    }

    #[test]
    fn parses_player_announcements() {
        let text = "[IP] 192.168.1.20 [Port] 55462 [Flags] 3 [Guid] 1846252609 [EditorId] 3141592653 \
                    [Version] 1048832 [Id] OSXPlayer(5,mac.local) [Debug] 1 [PackageName] OSXPlayer \
                    [ProjectName] Sample Game\0";
        let p = parse_announcement(text).unwrap();
        assert_eq!(p.ip, "192.168.1.20");
        assert_eq!(p.id, "OSXPlayer(5,mac.local)");
        assert_eq!(p.debugger_port, 56609);
        assert!(p.debug);
        assert_eq!(p.project.as_deref(), Some("Sample Game"));
        let row = p.into_attachable();
        assert_eq!(row.name, "Sample Game — OSXPlayer(5,mac.local)");
        assert_eq!((row.host.as_deref(), row.port), (Some("192.168.1.20"), Some(56609)));

        let explicit = "[IP] 10.0.0.2 [Port] 1 [Flags] 0 [Guid] 7 [EditorId] 1 [Version] 1 \
                        [Id] AndroidPlayer(Pixel):56123 [Debug] 0";
        let p = parse_announcement(explicit).unwrap();
        assert_eq!((p.id.as_str(), p.debugger_port, p.debug, p.project), ("AndroidPlayer(Pixel)", 56123, false, None));
        assert!(parse_announcement("hello").is_none());
    }

    /// With a Unity Editor open: `cargo test -p ide-dap --lib unity::tests::real_editors -- --ignored`.
    #[cfg(unix)]
    #[test]
    #[ignore]
    fn real_editors_are_found_and_listen() {
        let editors: Vec<_> = crate::attach::attachable_processes()
            .unwrap()
            .into_iter()
            .filter(|p| p.kind == AttachKind::UnityMono)
            .collect();
        assert!(!editors.is_empty(), "no Unity Editor is running");
        for e in &editors {
            eprintln!("{} pid {} port {:?}", e.name, e.pid, e.port);
            // Binding fails while the Editor listens. Connecting would wedge its debugger agent,
            // which serves one client and stalls after a failed handshake.
            let bound = std::net::TcpListener::bind(("127.0.0.1", e.port.unwrap()));
            assert!(bound.is_err(), "the Editor's debugger port listens");
        }
    }

    #[test]
    fn hears_a_player_on_the_multicast_group() {
        let sender = std::net::UdpSocket::bind("0.0.0.0:0").unwrap();
        let _ = sender.set_multicast_loop_v4(true);
        let msg = "[IP] 127.0.0.1 [Port] 1 [Flags] 3 [Guid] 4242 [EditorId] 1 [Version] 1 \
                   [Id] TestPlayer(1,here) [Debug] 1 [ProjectName] Fixture";
        let send = std::thread::spawn(move || {
            for _ in 0..8 {
                let _ = sender.send_to(msg.as_bytes(), (MULTICAST_GROUP, MULTICAST_PORTS[0]));
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        let players = discover_players(Duration::from_millis(900));
        send.join().unwrap();
        // Some sandboxes have no multicast route; the parser is covered above.
        if let Some(p) = players.iter().find(|p| p.pid == 4242) {
            assert_eq!(p.port, Some(56242));
            assert_eq!(p.name, "Fixture — TestPlayer(1,here)");
        }
    }
}
