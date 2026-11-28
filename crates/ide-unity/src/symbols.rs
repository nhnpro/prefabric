//! Unity project symbols for C# string-literal intelligence: tags, layers, sorting layers,
//! build scenes, legacy input axes, Animator parameters, and ShaderLab properties.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeSet;
use std::path::Path;

/// Built-in Unity tags always present in every project.
const BUILTIN_TAGS: &[&str] =
    &["Untagged", "Respawn", "Finish", "EditorOnly", "MainCamera", "Player", "GameController"];

/// Built-in Unity layers (indices 0, 1, 2, 4, 5).
const BUILTIN_LAYERS: &[&str] = &["Default", "TransparentFX", "Ignore Raycast", "Water", "UI"];

/// Cap per-asset-kind scans so huge projects stay sub-50ms.
const MAX_SCANNED_FILES: usize = 512;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UnityProjectSymbols {
    pub tags: Vec<String>,
    pub layers: Vec<String>,
    pub sorting_layers: Vec<String>,
    pub scenes: Vec<String>,
    pub input_axes: Vec<String>,
    pub animator_params: Vec<String>,
    pub shader_properties: Vec<String>,
}

/// Reads project-wide Unity string symbols from `ProjectSettings/` and `Assets/`.
pub fn collect(root: &Path) -> UnityProjectSymbols {
    let settings = root.join("ProjectSettings");
    let (tags, layers, sorting_layers) =
        parse_tag_manager(&std::fs::read_to_string(settings.join("TagManager.asset")).unwrap_or_default());
    let scenes =
        parse_build_settings(&std::fs::read_to_string(settings.join("EditorBuildSettings.asset")).unwrap_or_default());
    let input_axes =
        parse_input_manager(&std::fs::read_to_string(settings.join("InputManager.asset")).unwrap_or_default());
    let mut animator_params = BTreeSet::new();
    let mut shader_properties = BTreeSet::new();
    let assets = root.join("Assets");
    if assets.is_dir() {
        let mut controllers_left = MAX_SCANNED_FILES;
        let mut shaders_left = MAX_SCANNED_FILES;
        walk_assets(&assets, 0, &mut controllers_left, &mut shaders_left, &mut animator_params, &mut shader_properties);
    }
    UnityProjectSymbols {
        tags,
        layers,
        sorting_layers,
        scenes,
        input_axes,
        animator_params: animator_params.into_iter().collect(),
        shader_properties: shader_properties.into_iter().collect(),
    }
}

fn unquote(raw: &str) -> String {
    let s = raw.trim();
    if let Some(inner) = s.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        return inner.replace("''", "'");
    }
    if let Some(inner) = s.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        return inner.replace("\\\"", "\"").replace("\\\\", "\\");
    }
    s.to_owned()
}

pub(crate) fn parse_tag_manager(text: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut tags: BTreeSet<String> = BUILTIN_TAGS.iter().map(|s| (*s).to_owned()).collect();
    let mut layers: BTreeSet<String> = BUILTIN_LAYERS.iter().map(|s| (*s).to_owned()).collect();
    let mut sorting_layers: BTreeSet<String> = BTreeSet::from(["Default".to_owned()]);
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Section {
        None,
        Tags,
        Layers,
        SortingLayers,
    }
    let mut section = Section::None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if indent <= 2 && !trimmed.starts_with('-') {
            section = if trimmed.starts_with("tags:") {
                Section::Tags
            } else if trimmed.starts_with("layers:") {
                Section::Layers
            } else if trimmed.starts_with("m_SortingLayers:") {
                Section::SortingLayers
            } else {
                Section::None
            };
            continue;
        }
        match section {
            Section::Tags => {
                if let Some(rest) = trimmed.strip_prefix("- ") {
                    let v = unquote(rest);
                    if !v.is_empty() {
                        tags.insert(v);
                    }
                }
            }
            Section::Layers => {
                if let Some(rest) = trimmed.strip_prefix("- ") {
                    let v = unquote(rest);
                    if !v.is_empty() {
                        layers.insert(v);
                    }
                }
            }
            Section::SortingLayers => {
                let candidate = trimmed.strip_prefix("- ").unwrap_or(trimmed);
                if let Some(rest) = candidate.strip_prefix("name:") {
                    let v = unquote(rest);
                    if !v.is_empty() {
                        sorting_layers.insert(v);
                    }
                }
            }
            Section::None => {}
        }
    }
    (tags.into_iter().collect(), layers.into_iter().collect(), sorting_layers.into_iter().collect())
}

pub(crate) fn parse_build_settings(text: &str) -> Vec<String> {
    let mut scenes = BTreeSet::new();
    let mut in_scenes = false;
    let mut enabled = true;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if indent <= 2 && !trimmed.starts_with('-') {
            in_scenes = trimmed.starts_with("m_Scenes:");
            continue;
        }
        if !in_scenes {
            continue;
        }
        let item = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        if let Some(rest) = item.strip_prefix("enabled:") {
            enabled = rest.trim() != "0";
        } else if let Some(rest) = item.strip_prefix("path:") {
            if enabled {
                let path = unquote(rest).replace('\\', "/");
                if let Some(stem) = Path::new(&path).file_stem().and_then(|s| s.to_str())
                    && !stem.is_empty()
                {
                    scenes.insert(stem.to_owned());
                }
            }
            enabled = true;
        }
    }
    scenes.into_iter().collect()
}

pub(crate) fn parse_input_manager(text: &str) -> Vec<String> {
    let mut axes = BTreeSet::new();
    let mut in_axes = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if indent <= 2 && !trimmed.starts_with('-') {
            in_axes = trimmed.starts_with("m_Axes:");
            continue;
        }
        if !in_axes {
            continue;
        }
        let item = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        if let Some(rest) = item.strip_prefix("m_Name:") {
            let v = unquote(rest);
            if !v.is_empty() {
                axes.insert(v);
            }
        }
    }
    axes.into_iter().collect()
}

pub(crate) fn parse_animator_params(text: &str, out: &mut BTreeSet<String>) {
    let mut in_params = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if indent <= 2 && !trimmed.starts_with('-') {
            in_params = trimmed.starts_with("m_AnimatorParameters:");
            continue;
        }
        if !in_params {
            continue;
        }
        let item = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        if let Some(rest) = item.strip_prefix("m_Name:") {
            let v = unquote(rest);
            if !v.is_empty() {
                out.insert(v);
            }
        }
    }
}

pub(crate) fn parse_shader_properties(text: &str, out: &mut BTreeSet<String>) {
    let mut in_props = false;
    let mut depth = 0i32;
    for line in text.lines() {
        let code = line.split("//").next().unwrap_or("");
        let trimmed = code.trim();
        if !in_props {
            if let Some(rest) = trimmed.strip_prefix("Properties")
                && rest.trim_start().chars().next().is_none_or(|c| c == '{')
            {
                in_props = true;
                let opens = rest.chars().filter(|&c| c == '{').count() as i32;
                let closes = rest.chars().filter(|&c| c == '}').count() as i32;
                depth = opens - closes;
                // Handle single-line `Properties { _Foo ("Foo", Float) = 1 }` or `Properties {}`.
                if let Some(body) = rest.split_once('{').map(|(_, tail)| tail) {
                    extract_shader_prop(body.trim(), out);
                }
                if opens > 0 && depth <= 0 {
                    break;
                }
            }
            continue;
        }
        let opens = trimmed.chars().filter(|&c| c == '{').count() as i32;
        let closes = trimmed.chars().filter(|&c| c == '}').count() as i32;
        if depth > 0 || opens > 0 {
            extract_shader_prop(trimmed, out);
        }
        depth += opens - closes;
        if depth <= 0 && closes > 0 {
            break;
        }
    }
}

fn extract_shader_prop(trimmed: &str, out: &mut BTreeSet<String>) {
    // Skip leading `[Attr(...)]` brackets before `_PropName ("Label", Type) = ...`
    let mut s = trimmed;
    while let Some(rest) = s.strip_prefix('[') {
        let Some(end) = rest.find(']') else { break };
        s = rest[end + 1..].trim_start();
    }
    if let Some(paren) = s.find('(') {
        let name = s[..paren].trim();
        if !name.is_empty()
            && name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
            && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            out.insert(name.to_owned());
        }
    }
}

const MAX_WALK_DEPTH: usize = 12;

fn walk_assets(
    dir: &Path,
    depth: usize,
    controllers_left: &mut usize,
    shaders_left: &mut usize,
    animator_params: &mut BTreeSet<String>,
    shader_properties: &mut BTreeSet<String>,
) {
    if depth > MAX_WALK_DEPTH || (*controllers_left == 0 && *shaders_left == 0) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            let name = entry.file_name();
            if name.to_str().is_some_and(crate::walk::ignored) {
                continue;
            }
            walk_assets(&path, depth + 1, controllers_left, shaders_left, animator_params, shader_properties);
        } else if ft.is_file() {
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else { continue };
            if ext.eq_ignore_ascii_case("controller") && *controllers_left > 0 {
                *controllers_left -= 1;
                if entry.metadata().is_ok_and(|m| m.len() <= MAX_FILE_BYTES)
                    && let Ok(text) = std::fs::read_to_string(&path)
                {
                    parse_animator_params(&text, animator_params);
                }
            } else if ext.eq_ignore_ascii_case("shader") && *shaders_left > 0 {
                *shaders_left -= 1;
                if entry.metadata().is_ok_and(|m| m.len() <= MAX_FILE_BYTES)
                    && let Ok(text) = std::fs::read_to_string(&path)
                {
                    parse_shader_properties(&text, shader_properties);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tag_manager_tags_layers_and_sorting_layers() {
        let yaml = r#"%YAML 1.1
--- !u!78 &1
TagManager:
  tags:
  - Enemy
  - 'Boss Unit'
  layers:
  - Default
  -
  - Ground
  - Interactable
  m_SortingLayers:
  - name: Default
    uniqueID: 0
  - name: Foreground
    uniqueID: 123
"#;
        let (tags, layers, sorting) = parse_tag_manager(yaml);
        assert!(tags.contains(&"Player".to_owned()));
        assert!(tags.contains(&"Enemy".to_owned()));
        assert!(tags.contains(&"Boss Unit".to_owned()));
        assert!(layers.contains(&"UI".to_owned()));
        assert!(layers.contains(&"Ground".to_owned()));
        assert!(layers.contains(&"Interactable".to_owned()));
        assert_eq!(sorting, vec!["Default", "Foreground"]);
    }

    #[test]
    fn parses_build_settings_enabled_scenes_only() {
        let yaml = r#"%YAML 1.1
--- !u!1045 &1
EditorBuildSettings:
  m_Scenes:
  - enabled: 1
    path: Assets/Scenes/MainMenu.unity
  - enabled: 0
    path: Assets/Scenes/DisabledScene.unity
  - enabled: 1
    path: Assets/Scenes/Level 1.unity
"#;
        assert_eq!(parse_build_settings(yaml), vec!["Level 1", "MainMenu"]);
    }

    #[test]
    fn parses_input_axes_animator_params_and_shader_properties() {
        let input = r#"InputManager:
  m_Axes:
  - serializedVersion: 3
    m_Name: Horizontal
  - serializedVersion: 3
    m_Name: Jump
  - serializedVersion: 3
    m_Name: Horizontal
"#;
        assert_eq!(parse_input_manager(input), vec!["Horizontal", "Jump"]);

        let controller = r#"AnimatorController:
  m_AnimatorParameters:
  - m_Name: Speed
    m_Type: 1
  - m_Name: IsGrounded
    m_Type: 4
"#;
        let mut params = BTreeSet::new();
        parse_animator_params(controller, &mut params);
        assert_eq!(params.into_iter().collect::<Vec<_>>(), vec!["IsGrounded", "Speed"]);

        let shader = r#"Shader "Custom/Water" {
    Properties {
        [MainTexture] _BaseMap ("Albedo", 2D) = "white" {}
        [HDR] _TintColor ("Tint", Color) = (1,1,1,1)
        _WaveSpeed ("Wave Speed", Range(0, 10)) = 1.5
    }
    SubShader { Pass {} }
}"#;
        let mut props = BTreeSet::new();
        parse_shader_properties(shader, &mut props);
        assert_eq!(props.into_iter().collect::<Vec<_>>(), vec!["_BaseMap", "_TintColor", "_WaveSpeed"]);

        let mut empty_props = BTreeSet::new();
        parse_shader_properties(
            "Shader \"S\" { Properties {} SubShader { Pass { float foo(float x) { return x; } } } }",
            &mut empty_props,
        );
        assert!(empty_props.is_empty());
    }
}
