//! Assets the index can't read: a project saving binary assets (Asset Serialization Mode "Force
//! Binary" or "Mixed"), and assets left with merge-conflict markers. Both are reported to the user;
//! a conflicted asset is skipped until it is resolved.

use memchr::memmem;
use std::path::Path;

/// Why an asset isn't scanned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Problem {
    #[default]
    None,
    /// Serialized in Unity's binary format.
    Binary,
    /// Has merge-conflict markers.
    Conflict,
}

/// What a serialized asset's content allows.
pub(crate) fn problem(bytes: &[u8]) -> Problem {
    if !bytes.starts_with(b"%YAML") {
        return Problem::Binary;
    }
    let marker = |m: &[u8]| bytes.starts_with(m) || memmem::find(bytes, &[b"\n", m].concat()).is_some();
    if marker(b"<<<<<<< ") && marker(b">>>>>>> ") {
        return Problem::Conflict;
    }
    Problem::None
}

/// Asset Serialization Mode, from `ProjectSettings/EditorSettings.asset`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerializationMode {
    Mixed,
    ForceBinary,
    ForceText,
    /// The settings file is itself binary: the mode isn't Force Text.
    Unreadable,
}

pub fn serialization_mode(root: &Path) -> Option<SerializationMode> {
    let bytes = std::fs::read(root.join("ProjectSettings/EditorSettings.asset")).ok()?;
    if !bytes.starts_with(b"%YAML") {
        return Some(SerializationMode::Unreadable);
    }
    let text = String::from_utf8_lossy(&bytes);
    let mode = text.lines().find_map(|l| l.trim().strip_prefix("m_SerializationMode:"))?;
    Some(match mode.trim() {
        "0" => SerializationMode::Mixed,
        "1" => SerializationMode::ForceBinary,
        _ => SerializationMode::ForceText,
    })
}

pub(crate) const BINARY_TITLE: &str = "This Unity project saves assets in binary";
pub(crate) const BINARY_BODY: &str = "Prefabric reads scenes and prefabs as text to show where scripts, fields and methods are used. To switch, open Edit > Project Settings > Editor in Unity and set Asset Serialization > Mode to Force Text; Unity then saves every asset as text.";
pub(crate) const CONFLICT_TITLE: &str = "Unity assets with merge conflicts";

/// The conflict notice's body: the files, then how many more.
pub(crate) fn conflict_body(files: &[&str]) -> String {
    const SHOWN: usize = 5;
    let mut body = String::from("Not indexed until their conflicts are resolved:");
    for f in files.iter().take(SHOWN) {
        body.push('\n');
        body.push_str(f);
    }
    if files.len() > SHOWN {
        body.push_str(&format!("\n… and {} more", files.len() - SHOWN));
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_binary_and_conflicted_assets() {
        assert_eq!(problem(b"%YAML 1.1\n--- !u!1 &1\nGameObject:\n"), Problem::None);
        assert_eq!(problem(b"\0\0\0\0\x01\x02 binary"), Problem::Binary);
        assert_eq!(problem(b""), Problem::Binary);
        let conflicted =
            b"%YAML 1.1\n--- !u!1 &1\nGameObject:\n<<<<<<< HEAD\n  m_Name: A\n=======\n  m_Name: B\n>>>>>>> feature\n";
        assert_eq!(problem(conflicted), Problem::Conflict);
        // A name that merely contains the characters isn't a conflict.
        assert_eq!(problem(b"%YAML 1.1\n  m_Name: <<<<<<< x >>>>>>> y\n"), Problem::None);
    }

    #[test]
    fn reads_the_serialization_mode() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(serialization_mode(dir.path()), None);
        std::fs::create_dir_all(dir.path().join("ProjectSettings")).unwrap();
        let settings = dir.path().join("ProjectSettings/EditorSettings.asset");
        for (text, mode) in [
            ("%YAML 1.1\nEditorSettings:\n  m_SerializationMode: 2\n", SerializationMode::ForceText),
            ("%YAML 1.1\nEditorSettings:\n  m_SerializationMode: 1\n", SerializationMode::ForceBinary),
            ("%YAML 1.1\nEditorSettings:\n  m_SerializationMode: 0\n", SerializationMode::Mixed),
            ("\0binary", SerializationMode::Unreadable),
        ] {
            std::fs::write(&settings, text).unwrap();
            assert_eq!(serialization_mode(dir.path()), Some(mode));
        }
        assert!(conflict_body(&["a", "b", "c", "d", "e", "f", "g"]).ends_with("e\n… and 2 more"));
    }
}
