//! Installed Unity editors: Unity Hub's default folder and the custom one set in its
//! preferences (`secondaryInstallPath.json`).

use std::path::{Path, PathBuf};

/// The editor binary below a Hub version folder (`<hub>/<version>`).
fn binary_in(version_dir: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        version_dir.join("Unity.app/Contents/MacOS/Unity")
    } else if cfg!(windows) {
        version_dir.join("Editor").join("Unity.exe")
    } else {
        version_dir.join("Editor").join("Unity")
    }
}

/// Folders that hold one folder per installed version.
fn hub_dirs(home: &Path, config: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    // Unity Hub writes a JSON string, `""` when unset.
    if let Ok(text) = std::fs::read_to_string(config.join("UnityHub").join("secondaryInstallPath.json"))
        && let Ok(custom) = serde_json::from_str::<String>(text.trim())
        && !custom.is_empty()
    {
        dirs.push(PathBuf::from(custom));
    }
    if cfg!(target_os = "macos") {
        dirs.push(PathBuf::from("/Applications/Unity/Hub/Editor"));
    } else if cfg!(windows) {
        let program_files =
            std::env::var_os("ProgramFiles").map(PathBuf::from).unwrap_or_else(|| "C:\\Program Files".into());
        dirs.push(program_files.join("Unity").join("Hub").join("Editor"));
    } else {
        dirs.push(home.join("Unity").join("Hub").join("Editor"));
    }
    dirs
}

fn find_in(version: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    if version.is_empty() || version.contains(['/', '\\']) || version.contains("..") {
        return None;
    }
    dirs.iter().map(|dir| binary_in(&dir.join(version))).find(|binary| binary.is_file())
}

/// The editor of exactly `version` (as in `ProjectVersion.txt`): opening a project in
/// another version upgrades or downgrades it, which is not for the IDE to do.
pub fn find(version: &str) -> Option<PathBuf> {
    let home = dirs_home()?;
    find_in(version, &hub_dirs(&home, &config_dir(&home)))
}

/// Unity's scene and prefab merge tool (`UnityYAMLMerge`): the one of `version` when installed,
/// else the newest installed one's (the tool barely changes between versions).
pub fn yaml_merge(version: Option<&str>) -> Option<PathBuf> {
    let home = dirs_home()?;
    yaml_merge_in(version, &hub_dirs(&home, &config_dir(&home)))
}

fn yaml_merge_in(version: Option<&str>, dirs: &[PathBuf]) -> Option<PathBuf> {
    let tool = |editor: PathBuf| {
        let contents = editor.parent()?;
        let tool = if cfg!(target_os = "macos") {
            contents.parent()?.join("Helpers/UnityYAMLMerge")
        } else {
            contents.join("Data").join("Tools").join(if cfg!(windows) {
                "UnityYAMLMerge.exe"
            } else {
                "UnityYAMLMerge"
            })
        };
        tool.is_file().then_some(tool)
    };
    if let Some(found) = version.and_then(|v| find_in(v, dirs)).and_then(tool) {
        return Some(found);
    }
    let mut versions: Vec<PathBuf> =
        dirs.iter().filter_map(|dir| std::fs::read_dir(dir).ok()).flatten().flatten().map(|e| e.path()).collect();
    versions.sort_by_key(|p| std::cmp::Reverse(version_key(p)));
    versions.into_iter().find_map(|dir| tool(binary_in(&dir)))
}

/// `6000.6.8f1` → `[6000, 6, 8, 1]`: numeric order, so 2022.3.10 sorts after 2022.3.9.
fn version_key(dir: &Path) -> Vec<u64> {
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    name.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).map(|s| s.parse().unwrap_or(0)).collect()
}

fn config_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else if cfg!(windows) {
        // Without APPDATA, only Unity Hub's custom folder is unknown; the default one is still searched.
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData").join("Roaming"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"))
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_exact_version_in_the_custom_hub_folder_first() {
        let temp = tempfile::tempdir().unwrap();
        let (home, config, custom) =
            (temp.path().join("home"), temp.path().join("config"), temp.path().join("Editors"));
        std::fs::create_dir_all(config.join("UnityHub")).unwrap();
        std::fs::write(config.join("UnityHub/secondaryInstallPath.json"), serde_json::to_string(&custom).unwrap())
            .unwrap();
        let binary = binary_in(&custom.join("2022.3.62f1"));
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        std::fs::write(&binary, "").unwrap();
        let dirs = hub_dirs(&home, &config);
        assert_eq!(dirs[0], custom);
        assert_eq!(find_in("2022.3.62f1", &dirs), Some(binary));
        assert_eq!(find_in("2022.3.61f1", &dirs), None);
        assert_eq!(find_in("../2022.3.62f1", &dirs), None);
        assert_eq!(find_in("", &dirs), None);
        std::fs::write(config.join("UnityHub/secondaryInstallPath.json"), "\"\"").unwrap();
        assert_eq!(hub_dirs(&home, &config).len(), 1);
    }

    #[test]
    fn finds_the_merge_tool_of_the_version_or_the_newest_one() {
        let temp = tempfile::tempdir().unwrap();
        let hub = temp.path().to_path_buf();
        let install = |version: &str| {
            let editor = binary_in(&hub.join(version));
            std::fs::create_dir_all(editor.parent().unwrap()).unwrap();
            std::fs::write(&editor, "").unwrap();
            let tool = if cfg!(target_os = "macos") {
                editor.parent().unwrap().parent().unwrap().join("Helpers/UnityYAMLMerge")
            } else {
                editor.parent().unwrap().join("Data/Tools").join(if cfg!(windows) {
                    "UnityYAMLMerge.exe"
                } else {
                    "UnityYAMLMerge"
                })
            };
            std::fs::create_dir_all(tool.parent().unwrap()).unwrap();
            std::fs::write(&tool, "").unwrap();
            tool
        };
        let dirs = [hub.clone()];
        assert_eq!(yaml_merge_in(None, &dirs), None);
        let old = install("2022.3.9f1");
        let newer = install("2022.3.10f1");
        assert_eq!(yaml_merge_in(Some("2022.3.9f1"), &dirs), Some(old));
        assert_eq!(yaml_merge_in(Some("6000.0.1f1"), &dirs), Some(newer.clone()));
        assert_eq!(yaml_merge_in(None, &dirs), Some(newer));
    }
}
