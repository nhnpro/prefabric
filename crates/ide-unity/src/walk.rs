//! The assets of a Unity project: every file under `Assets/` and the embedded packages in
//! `Packages/`, with the GUID of its `.meta` file. Registry packages (`Library/PackageCache`) are
//! read-only and not indexed.

use crate::scan::Guid;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Serialized assets whose objects are scanned; other assets only map their GUID to their path.
const SCANNED: &[&str] =
    &["unity", "prefab", "asset", "anim", "controller", "overridecontroller", "playable", "mask", "mixer", "lighting"];

pub(crate) struct Entry {
    /// Relative to the project root, with `/`.
    pub rel: String,
    pub abs: PathBuf,
}

/// Size and modification time of a file, to see whether it changed since it was indexed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Stamp {
    pub len: u64,
    pub modified_ns: u64,
}

impl Stamp {
    pub(crate) fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        let modified_ns = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX));
        Some(Self { len: meta.len(), modified_ns })
    }
}

/// Unity skips hidden files and folders, and those ending in `~` (`Documentation~`).
pub(crate) fn ignored(name: &str) -> bool {
    name.starts_with('.') || name.ends_with('~') || name.eq_ignore_ascii_case("cvs") || name.ends_with(".tmp")
}

pub(crate) fn is_scanned(rel: &str) -> bool {
    // Baked lighting is binary even in Force Text projects, and holds no script references.
    let name = rel.rsplit('/').next().unwrap_or(rel);
    if name.ends_with("LightingData.asset") {
        return false;
    }
    rel.rsplit_once('.').is_some_and(|(_, ext)| SCANNED.iter().any(|s| ext.eq_ignore_ascii_case(s)))
}

/// Every asset file (not `.meta`, not folders) under the asset roots.
pub(crate) fn assets(root: &Path) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut stack: Vec<(PathBuf, String)> = vec![(root.join("Assets"), "Assets".into())];
    // Embedded packages are folders; `manifest.json` and lock files are not assets.
    if let Ok(dir) = std::fs::read_dir(root.join("Packages")) {
        for entry in dir.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !ignored(&name) && entry.file_type().is_ok_and(|t| t.is_dir()) {
                stack.push((entry.path(), format!("Packages/{name}")));
            }
        }
    }
    while let Some((dir, rel)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if ignored(&name) {
                continue;
            }
            let child = format!("{rel}/{name}");
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push((entry.path(), child)),
                Ok(t) if t.is_file() && !name.ends_with(".meta") => out.push(Entry { rel: child, abs: entry.path() }),
                _ => {}
            }
        }
    }
    out
}

/// Asset files below the folder `dir` (`rel` relative to the root).
pub(crate) fn files_under(dir: &Path, rel: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_owned(), rel.to_owned())];
    while let Some((dir, rel)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if ignored(&name) {
                continue;
            }
            let child = format!("{rel}/{name}");
            match entry.file_type() {
                Ok(t) if t.is_dir() => stack.push((entry.path(), child)),
                Ok(t) if t.is_file() && !name.ends_with(".meta") => out.push(child),
                _ => {}
            }
        }
    }
    out
}

pub(crate) fn meta_path(asset: &Path) -> PathBuf {
    let mut name = asset.as_os_str().to_os_string();
    name.push(".meta");
    PathBuf::from(name)
}

/// The `guid:` line of a `.meta` file.
pub(crate) fn meta_guid(text: &[u8]) -> Option<Guid> {
    text.split(|&b| b == b'\n').find_map(|line| Guid::parse(line.strip_prefix(b"guid: ")?.trim_ascii()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_assets_and_embedded_packages() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for d in
            ["Assets/A", "Assets/.hidden", "Assets/Docs~", "Packages/com.me.tools/Runtime", "Library/PackageCache/x"]
        {
            std::fs::create_dir_all(root.join(d)).unwrap();
        }
        for f in [
            "Assets/A/Hero.prefab",
            "Assets/A/Hero.prefab.meta",
            "Assets/.hidden/x.prefab",
            "Assets/Docs~/y.prefab",
            "Assets/A/.DS_Store",
            "Packages/manifest.json",
            "Packages/com.me.tools/Runtime/Tool.cs",
            "Library/PackageCache/x/z.prefab",
        ] {
            std::fs::write(root.join(f), "").unwrap();
        }
        let mut rels: Vec<_> = assets(root).into_iter().map(|e| e.rel).collect();
        rels.sort();
        assert_eq!(rels, ["Assets/A/Hero.prefab", "Packages/com.me.tools/Runtime/Tool.cs"]);
    }

    #[test]
    fn reads_meta_guids_and_scanned_kinds() {
        let meta = b"fileFormatVersion: 2\r\nguid: 79cb0651700894f0a8e50458c76fa759\r\nPrefabImporter:\n";
        assert_eq!(meta_guid(meta).unwrap().to_string(), "79cb0651700894f0a8e50458c76fa759");
        assert_eq!(meta_guid(b"guid: nope\n"), None);
        assert!(
            is_scanned("Assets/S.unity") && is_scanned("Assets/a.PREFAB") && is_scanned("Assets/x.overrideController")
        );
        assert!(!is_scanned("Assets/P.cs") && !is_scanned("Assets/noext"));
        assert!(!is_scanned("Assets/Scenes/Main/LightingData.asset") && is_scanned("Assets/Settings.asset"));
    }
}
