//! Merging scenes, prefabs and other YAML assets with Unity's `UnityYAMLMerge`, which merges
//! them by object and property instead of by line.

use ide_core::{IdeError, IdeResult};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;
use std::process::Command;

/// Assets Unity serializes as YAML (with Force Text serialization, the default).
const EXTENSIONS: &[&str] = &[
    "unity",
    "prefab",
    "asset",
    "mat",
    "anim",
    "controller",
    "overridecontroller",
    "playable",
    "mask",
    "physicmaterial",
    "physicsmaterial2d",
    "lighting",
    "preset",
    "spriteatlas",
    "mixer",
    "rendertexture",
    "terrainlayer",
    "signal",
];

/// Whether `path` is an asset UnityYAMLMerge can merge (by its extension).
pub fn is_yaml_asset(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|known| known.eq_ignore_ascii_case(e)))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct YamlMerge {
    /// The merged asset; `None` when properties changed on both sides.
    pub merged: Option<String>,
    /// The properties changed on both sides (`<fileID>.<Type>.<property>`), when `merged` is `None`.
    pub conflicts: Vec<String>,
}

/// Merges three versions of the asset `name` (its extension tells the tool the kind) with `tool`.
pub fn merge(tool: &Path, name: &str, base: &str, ours: &str, theirs: &str) -> IdeResult<YamlMerge> {
    let dir = tempfile::tempdir()?;
    let ext = Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("asset");
    let file = |stem: &str, text: &str| -> IdeResult<std::path::PathBuf> {
        let path = dir.path().join(format!("{stem}.{ext}"));
        std::fs::write(&path, text)?;
        Ok(path)
    };
    let (base, ours, theirs) = (file("base", base)?, file("yours", ours)?, file("theirs", theirs)?);
    let dest = dir.path().join(format!("merged.{ext}"));
    // Left is theirs and right is yours. Headless, no fallback tool: conflicts are reported.
    let out = Command::new(tool)
        .args(["merge", "-h", "-p", "--fallback", "none"])
        .args([&base, &theirs, &ours, &dest])
        .current_dir(dir.path())
        .output()
        .map_err(|e| IdeError::unavailable(format!("Cannot run UnityYAMLMerge: {e}")))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if out.status.success() {
        let merged = std::fs::read_to_string(&dest)
            .map_err(|e| IdeError::internal(format!("UnityYAMLMerge wrote no result: {e}")))?;
        return Ok(YamlMerge { merged: Some(merged), conflicts: Vec::new() });
    }
    let conflicts = conflicts(&stdout);
    if conflicts.is_empty() {
        let detail = format!("{stdout}{}", String::from_utf8_lossy(&out.stderr));
        return Err(IdeError::internal(format!("UnityYAMLMerge failed: {}", detail.trim())));
    }
    Ok(YamlMerge { merged: None, conflicts })
}

/// `Left  6979874606233519545.CapsuleCollider.m_Radius change to 0.9` → the property, once.
fn conflicts(stdout: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("Left ").or_else(|| line.strip_prefix("Right ")) else { continue };
        let property = rest.trim_start().split(' ').next().unwrap_or_default();
        if !property.is_empty() && !found.iter().any(|f| f == property) {
            found.push(property.to_owned());
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_yaml_assets_by_extension() {
        assert!(is_yaml_asset(Path::new("Assets/Scenes/Main.unity")));
        assert!(is_yaml_asset(Path::new("Assets/P.PREFAB")));
        assert!(is_yaml_asset(Path::new("Assets/M.physicMaterial")));
        assert!(!is_yaml_asset(Path::new("Assets/Player.cs")));
        assert!(!is_yaml_asset(Path::new("Assets/Player.prefab.meta")));
    }

    #[test]
    fn reads_the_conflicting_properties() {
        let out = "Conflicts:\nLeft  6979.CapsuleCollider.m_Radius change to 0.9\nRight 6979.CapsuleCollider.m_Radius change to 0.7\nLeft  12.Transform.m_LocalPosition.x change to 1\nConflict handling:\n";
        assert_eq!(conflicts(out), ["6979.CapsuleCollider.m_Radius", "12.Transform.m_LocalPosition.x"]);
        assert!(conflicts("Conflicts:\nConflict handling:\n").is_empty());
    }

    /// With Unity installed: changes to neighbouring lines merge, the same property conflicts.
    #[test]
    fn merges_with_the_installed_tool() {
        let Some(tool) = super::super::editor::yaml_merge(None) else { return };
        let base = "%YAML 1.1\n%TAG !u! tag:unity3d.com,2011:\n--- !u!136 &6979\nCapsuleCollider:\n  m_ObjectHideFlags: 0\n  m_Radius: 0.5\n  m_Height: 1\n";
        let ours = base.replace("m_Radius: 0.5", "m_Radius: 0.7");
        let theirs = base.replace("m_Height: 1", "m_Height: 2");
        let merged = merge(&tool, "P.prefab", base, &ours, &theirs).unwrap();
        assert_eq!(merged.merged.as_deref(), Some(ours.replace("m_Height: 1", "m_Height: 2").as_str()));
        let theirs = base.replace("m_Radius: 0.5", "m_Radius: 0.9");
        let conflict = merge(&tool, "P.prefab", base, &ours, &theirs).unwrap();
        assert_eq!(conflict, YamlMerge { merged: None, conflicts: vec!["6979.CapsuleCollider.m_Radius".into()] });
    }
}
