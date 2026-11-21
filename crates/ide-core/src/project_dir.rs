//! The per-project config folder (`<root>/.prefabric/`). Projects set up before the
//! rename keep their `<root>/.openidea/` folder: it is used as long as there is no
//! `.prefabric/` next to it.

use std::path::{Path, PathBuf};

/// Folder name under the project root.
pub const DIR: &str = ".prefabric";
/// The folder name of earlier builds, still read and written when [`DIR`] is absent.
pub const LEGACY_DIR: &str = ".openidea";
/// Both folder names, for code that skips or recognises the folder in any project.
pub const NAMES: [&str; 2] = [DIR, LEGACY_DIR];

/// The folder name `root` uses: [`LEGACY_DIR`] when only that one exists, else [`DIR`].
pub fn name(root: &Path) -> &'static str {
    if root.join(DIR).exists() || !root.join(LEGACY_DIR).is_dir() { DIR } else { LEGACY_DIR }
}

/// The config folder of `root`.
pub fn dir(root: &Path) -> PathBuf {
    root.join(name(root))
}

/// Where `rel` (e.g. `settings.toml`) is read from and written to.
pub fn path(root: &Path, rel: impl AsRef<Path>) -> PathBuf {
    dir(root).join(rel)
}

/// Whether a file name is the config folder's (either name).
pub fn is_name(name: &str) -> bool {
    NAMES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_projects_use_prefabric_and_old_ones_keep_openidea() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        assert_eq!(path(root, "settings.toml"), root.join(".prefabric/settings.toml"));

        std::fs::create_dir(root.join(".openidea")).unwrap();
        assert_eq!(name(root), ".openidea");
        assert_eq!(path(root, "settings.toml"), root.join(".openidea/settings.toml"));

        std::fs::create_dir(root.join(".prefabric")).unwrap();
        assert_eq!(dir(root), root.join(".prefabric"), "the new folder wins once it exists");
        assert!(is_name(".openidea") && is_name(".prefabric") && !is_name(".idea"));
    }
}
