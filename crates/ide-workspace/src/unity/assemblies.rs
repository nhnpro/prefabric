//! The assembly definitions (`.asmdef`) of a Unity project and its packages, with the GUID that
//! `GUID:` references name (from the `.meta` file).

use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};

/// Folders searched, relative to the Unity root: the project's assets, embedded and local
/// packages, and the downloaded packages Unity resolved.
const ROOTS: [&str; 3] = ["Assets", "Packages", "Library/PackageCache"];
const MAX_DEPTH: usize = 12;
const LIMIT: usize = 5_000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AssemblyDefinition {
    pub name: String,
    pub path: PathBuf,
    /// From the `.meta` file; none when Unity hasn't imported it yet.
    pub guid: Option<String>,
}

pub fn assembly_definitions(root: &Path) -> Vec<AssemblyDefinition> {
    let mut out = Vec::new();
    for dir in ROOTS {
        find(&root.join(dir), 0, &mut out);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
    out
}

fn find(dir: &Path, depth: usize, out: &mut Vec<AssemblyDefinition>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if out.len() >= LIMIT {
            return;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Ok(kind) = entry.file_type() else { continue };
        let path = entry.path();
        if kind.is_dir() {
            // Unity ignores hidden folders and folders ending in `~`.
            if depth < MAX_DEPTH && !name.starts_with('.') && !name.ends_with('~') {
                find(&path, depth + 1, out);
            }
        } else if name.ends_with(".asmdef")
            && let Some(definition) = read(&path)
        {
            out.push(definition);
        }
    }
}

fn read(path: &Path) -> Option<AssemblyDefinition> {
    let text = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    let name = json.get("name")?.as_str()?.to_owned();
    Some(AssemblyDefinition { name, path: path.to_path_buf(), guid: guid(&super::assets::meta_path(path)) })
}

fn guid(meta: &Path) -> Option<String> {
    let text = std::fs::read_to_string(meta).ok()?;
    text.lines().find_map(|l| l.strip_prefix("guid:")).map(|g| g.trim().to_owned()).filter(|g| !g.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn finds_definitions_in_assets_and_packages_with_their_guids() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("Assets/Scripts/Game.asmdef"), "\u{feff}{ \"name\": \"Game\" }");
        write(&root.join("Assets/Scripts/Game.asmdef.meta"), "fileFormatVersion: 2\nguid: 0123abcd\n");
        write(&root.join("Library/PackageCache/com.x@1/Runtime/X.asmdef"), "{ \"name\": \"X.Runtime\" }");
        write(&root.join("Assets/Samples~/S.asmdef"), "{ \"name\": \"Ignored\" }");
        write(&root.join("Assets/Broken.asmdef"), "{ name");
        write(&root.join("Temp/T.asmdef"), "{ \"name\": \"Temp\" }");
        let found = assembly_definitions(root);
        assert_eq!(
            found,
            [
                AssemblyDefinition {
                    name: "Game".into(),
                    path: root.join("Assets/Scripts/Game.asmdef"),
                    guid: Some("0123abcd".into())
                },
                AssemblyDefinition {
                    name: "X.Runtime".into(),
                    path: root.join("Library/PackageCache/com.x@1/Runtime/X.asmdef"),
                    guid: None
                },
            ]
        );
    }
}
