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
    let config = if cfg!(target_os = "macos") {
        home.join("Library/Application Support")
    } else if cfg!(windows) {
        // Without APPDATA, only Unity Hub's custom folder is unknown; the default one is still searched.
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData").join("Roaming"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"))
    };
    find_in(version, &hub_dirs(&home, &config))
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
}
