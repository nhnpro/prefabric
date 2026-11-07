//! Unity project identity. Generated C# solutions are optional, not root markers.

pub(crate) mod assets;
pub mod editor;
pub mod package;

use serde::{Deserialize, Serialize};
use specta::Type;
use std::io::Read;
use std::path::Path;

pub const VERSION_FILE: &str = "ProjectSettings/ProjectVersion.txt";
/// Root-relative caches and build outputs, including folders not created yet.
pub const GENERATED_DIRS: &[&str] = &["Library", "Temp", "Obj", "obj", "Logs", "Build", "Builds", "UserSettings"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UnityProjectInfo {
    pub editor_version: Option<String>,
    /// File names relative to the Unity root, in stable order.
    pub solutions: Vec<String>,
    pub projects: Vec<String>,
    /// Prefabric's Unity package in this project.
    #[serde(default)]
    #[specta(optional)]
    pub package: Option<UnityPackageStatus>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum UnityPackageStatus {
    Missing,
    /// The embedded copy is the app's version (or newer).
    Current,
    /// The embedded copy is older than the app's.
    Outdated {
        installed: String,
    },
    /// Listed in `Packages/manifest.json`; updated by hand.
    Manual {
        source: String,
    },
}

fn package_status(root: &Path) -> UnityPackageStatus {
    let numbers = |v: &str| -> Vec<u64> {
        v.split(['-', '+']).next().unwrap_or_default().split('.').map(|n| n.parse().unwrap_or(0)).collect()
    };
    match package::installed(root) {
        None => UnityPackageStatus::Missing,
        Some(package::Installed::Manifest(source)) => UnityPackageStatus::Manual { source },
        Some(package::Installed::Embedded(version)) if numbers(&version) < numbers(package::BUNDLED_VERSION) => {
            UnityPackageStatus::Outdated { installed: version }
        }
        Some(package::Installed::Embedded(_)) => UnityPackageStatus::Current,
    }
}

pub fn is_project(root: &Path) -> bool {
    root.join("Assets").is_dir() && root.join(VERSION_FILE).is_file()
}

pub fn detect(root: &Path) -> Option<UnityProjectInfo> {
    if !is_project(root) {
        return None;
    }
    let mut text = String::new();
    let _ = std::fs::File::open(root.join(VERSION_FILE)).and_then(|f| f.take(64 * 1024).read_to_string(&mut text));
    let editor_version = text.lines().find_map(|line| {
        let (key, value) = line.trim_start_matches('\u{feff}').split_once(':')?;
        let value = value.trim();
        (key.trim() == "m_EditorVersion" && !value.is_empty() && value.len() <= 128).then(|| value.to_owned())
    });
    let mut info = UnityProjectInfo {
        editor_version,
        solutions: Vec::new(),
        projects: Vec::new(),
        package: Some(package_status(root)),
    };
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            match entry.path().extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
                Some("sln" | "slnx") => info.solutions.push(name),
                Some("csproj") => info.projects.push(name),
                _ => {}
            }
        }
    }
    info.solutions.sort();
    info.projects.sort();
    Some(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_fresh_and_legacy_projects_without_packages_or_solutions() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir(root.join("Assets")).unwrap();
        assert!(detect(root).is_none());
        std::fs::create_dir(root.join("ProjectSettings")).unwrap();
        std::fs::write(
            root.join(VERSION_FILE),
            "\u{feff}m_EditorVersion: 2022.3.62f1\r\nm_EditorVersionWithRevision: ignored\r\n",
        )
        .unwrap();
        let info = detect(root).unwrap();
        assert_eq!(info.editor_version.as_deref(), Some("2022.3.62f1"));
        assert!(info.solutions.is_empty() && info.projects.is_empty());
        std::fs::write(root.join("Game.sln"), "").unwrap();
        std::fs::write(root.join("Game.slnx"), "").unwrap();
        std::fs::write(root.join("Assembly-CSharp.csproj"), "").unwrap();
        std::fs::create_dir(root.join("NotAProject.csproj")).unwrap();
        let info = detect(root).unwrap();
        assert_eq!(info.solutions, ["Game.sln", "Game.slnx"]);
        assert_eq!(info.projects, ["Assembly-CSharp.csproj"]);
        std::fs::write(root.join(VERSION_FILE), "malformed").unwrap();
        assert_eq!(detect(root).unwrap().editor_version, None);
    }

    #[test]
    fn reports_the_package_state() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("Assets")).unwrap();
        std::fs::create_dir_all(root.join("ProjectSettings")).unwrap();
        std::fs::create_dir_all(root.join("Packages")).unwrap();
        std::fs::write(root.join(VERSION_FILE), "m_EditorVersion: 6000.6.8f1\n").unwrap();
        std::fs::write(root.join("Packages/manifest.json"), r#"{"dependencies":{}}"#).unwrap();
        let state = || detect(root).unwrap().package.unwrap();
        assert_eq!(state(), UnityPackageStatus::Missing);
        package::install(root).unwrap();
        assert_eq!(state(), UnityPackageStatus::Current);
        let manifest = package::embedded_dir(root).join("package.json");
        for (version, expected) in [
            ("0.0.9", UnityPackageStatus::Outdated { installed: "0.0.9".into() }),
            ("99.0.0", UnityPackageStatus::Current),
            (package::BUNDLED_VERSION, UnityPackageStatus::Current),
        ] {
            std::fs::write(&manifest, format!(r#"{{"version":"{version}"}}"#)).unwrap();
            assert_eq!(state(), expected, "{version}");
        }
        std::fs::remove_dir_all(package::embedded_dir(root)).unwrap();
        std::fs::write(
            root.join("Packages/manifest.json"),
            r#"{"dependencies":{"dev.prefabric.unity":"file:../prefabric/unity"}}"#,
        )
        .unwrap();
        assert_eq!(state(), UnityPackageStatus::Manual { source: "file:../prefabric/unity".into() });
    }

    #[test]
    fn ordinary_csharp_projects_are_not_unity_projects() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("App.csproj"), "").unwrap();
        std::fs::create_dir(temp.path().join("Assets")).unwrap();
        assert!(detect(temp.path()).is_none());
    }
}
