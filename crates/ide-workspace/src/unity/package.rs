//! Prefabric's Unity package (`unity/`, `dev.prefabric.unity`), carried by the app and copied
//! into a Unity project's `Packages/` as an embedded package: no absolute paths in the
//! project, it works for the whole team, and the IDE can update it.

use std::io;
use std::path::{Path, PathBuf};

pub const NAME: &str = "dev.prefabric.unity";
/// `version` of `unity/package.json`.
pub const BUNDLED_VERSION: &str = "0.1.1";

/// Every file of `unity/` (a test keeps the list complete).
const FILES: &[(&str, &[u8])] = &[
    (
        "Analyzers~/Microsoft.Unity.Analyzers.dll",
        include_bytes!("../../../../unity/Analyzers~/Microsoft.Unity.Analyzers.dll"),
    ),
    ("CHANGELOG.md", include_bytes!("../../../../unity/CHANGELOG.md")),
    ("CHANGELOG.md.meta", include_bytes!("../../../../unity/CHANGELOG.md.meta")),
    ("Editor.meta", include_bytes!("../../../../unity/Editor.meta")),
    ("Editor/AssemblyInfo.cs", include_bytes!("../../../../unity/Editor/AssemblyInfo.cs")),
    ("Editor/AssemblyInfo.cs.meta", include_bytes!("../../../../unity/Editor/AssemblyInfo.cs.meta")),
    ("Editor/FileUtility.cs", include_bytes!("../../../../unity/Editor/FileUtility.cs")),
    ("Editor/FileUtility.cs.meta", include_bytes!("../../../../unity/Editor/FileUtility.cs.meta")),
    ("Editor/MSBuildProjectIntegration.cs", include_bytes!("../../../../unity/Editor/MSBuildProjectIntegration.cs")),
    (
        "Editor/MSBuildProjectIntegration.cs.meta",
        include_bytes!("../../../../unity/Editor/MSBuildProjectIntegration.cs.meta"),
    ),
    ("Editor/Prefabric.Unity.Editor.asmdef", include_bytes!("../../../../unity/Editor/Prefabric.Unity.Editor.asmdef")),
    (
        "Editor/Prefabric.Unity.Editor.asmdef.meta",
        include_bytes!("../../../../unity/Editor/Prefabric.Unity.Editor.asmdef.meta"),
    ),
    ("Editor/PrefabricEditor.cs", include_bytes!("../../../../unity/Editor/PrefabricEditor.cs")),
    ("Editor/PrefabricEditor.cs.meta", include_bytes!("../../../../unity/Editor/PrefabricEditor.cs.meta")),
    ("Editor/PrefabricInstallation.cs", include_bytes!("../../../../unity/Editor/PrefabricInstallation.cs")),
    ("Editor/PrefabricInstallation.cs.meta", include_bytes!("../../../../unity/Editor/PrefabricInstallation.cs.meta")),
    ("Editor/ProjectFiles.cs", include_bytes!("../../../../unity/Editor/ProjectFiles.cs")),
    ("Editor/ProjectFiles.cs.meta", include_bytes!("../../../../unity/Editor/ProjectFiles.cs.meta")),
    ("Editor/ProcessRunner.cs", include_bytes!("../../../../unity/Editor/ProcessRunner.cs")),
    ("Editor/ProcessRunner.cs.meta", include_bytes!("../../../../unity/Editor/ProcessRunner.cs.meta")),
    ("Editor/ProjectGeneration.meta", include_bytes!("../../../../unity/Editor/ProjectGeneration.meta")),
    (
        "Editor/ProjectGeneration/AssemblyNameProvider.cs",
        include_bytes!("../../../../unity/Editor/ProjectGeneration/AssemblyNameProvider.cs"),
    ),
    (
        "Editor/ProjectGeneration/AssemblyNameProvider.cs.meta",
        include_bytes!("../../../../unity/Editor/ProjectGeneration/AssemblyNameProvider.cs.meta"),
    ),
    (
        "Editor/ProjectGeneration/FileIOProvider.cs",
        include_bytes!("../../../../unity/Editor/ProjectGeneration/FileIOProvider.cs"),
    ),
    (
        "Editor/ProjectGeneration/FileIOProvider.cs.meta",
        include_bytes!("../../../../unity/Editor/ProjectGeneration/FileIOProvider.cs.meta"),
    ),
    (
        "Editor/ProjectGeneration/GUIDProvider.cs",
        include_bytes!("../../../../unity/Editor/ProjectGeneration/GUIDProvider.cs"),
    ),
    (
        "Editor/ProjectGeneration/GUIDProvider.cs.meta",
        include_bytes!("../../../../unity/Editor/ProjectGeneration/GUIDProvider.cs.meta"),
    ),
    (
        "Editor/ProjectGeneration/ProjectGeneration.cs",
        include_bytes!("../../../../unity/Editor/ProjectGeneration/ProjectGeneration.cs"),
    ),
    (
        "Editor/ProjectGeneration/ProjectGeneration.cs.meta",
        include_bytes!("../../../../unity/Editor/ProjectGeneration/ProjectGeneration.cs.meta"),
    ),
    (
        "Editor/ProjectGeneration/ProjectGenerationFlag.cs",
        include_bytes!("../../../../unity/Editor/ProjectGeneration/ProjectGenerationFlag.cs"),
    ),
    (
        "Editor/ProjectGeneration/ProjectGenerationFlag.cs.meta",
        include_bytes!("../../../../unity/Editor/ProjectGeneration/ProjectGenerationFlag.cs.meta"),
    ),
    (
        "Editor/ProjectGeneration/ProjectProperties.cs",
        include_bytes!("../../../../unity/Editor/ProjectGeneration/ProjectProperties.cs"),
    ),
    (
        "Editor/ProjectGeneration/ProjectProperties.cs.meta",
        include_bytes!("../../../../unity/Editor/ProjectGeneration/ProjectProperties.cs.meta"),
    ),
    (
        "Editor/ProjectGeneration/SdkStyleProjectGeneration.cs",
        include_bytes!("../../../../unity/Editor/ProjectGeneration/SdkStyleProjectGeneration.cs"),
    ),
    (
        "Editor/ProjectGeneration/SdkStyleProjectGeneration.cs.meta",
        include_bytes!("../../../../unity/Editor/ProjectGeneration/SdkStyleProjectGeneration.cs.meta"),
    ),
    (
        "Editor/ProjectGeneration/TypeCacheHelper.cs",
        include_bytes!("../../../../unity/Editor/ProjectGeneration/TypeCacheHelper.cs"),
    ),
    (
        "Editor/ProjectGeneration/TypeCacheHelper.cs.meta",
        include_bytes!("../../../../unity/Editor/ProjectGeneration/TypeCacheHelper.cs.meta"),
    ),
    ("Editor/Solution.cs", include_bytes!("../../../../unity/Editor/Solution.cs")),
    ("Editor/Solution.cs.meta", include_bytes!("../../../../unity/Editor/Solution.cs.meta")),
    ("Editor/SolutionParser.cs", include_bytes!("../../../../unity/Editor/SolutionParser.cs")),
    ("Editor/SolutionParser.cs.meta", include_bytes!("../../../../unity/Editor/SolutionParser.cs.meta")),
    ("Editor/SolutionProjectEntry.cs", include_bytes!("../../../../unity/Editor/SolutionProjectEntry.cs")),
    ("Editor/SolutionProjectEntry.cs.meta", include_bytes!("../../../../unity/Editor/SolutionProjectEntry.cs.meta")),
    ("Editor/SolutionProperties.cs", include_bytes!("../../../../unity/Editor/SolutionProperties.cs")),
    ("Editor/SolutionProperties.cs.meta", include_bytes!("../../../../unity/Editor/SolutionProperties.cs.meta")),
    ("Editor/UnityInstallation.cs", include_bytes!("../../../../unity/Editor/UnityInstallation.cs")),
    ("Editor/UnityInstallation.cs.meta", include_bytes!("../../../../unity/Editor/UnityInstallation.cs.meta")),
    ("LICENSE.md", include_bytes!("../../../../unity/LICENSE.md")),
    ("LICENSE.md.meta", include_bytes!("../../../../unity/LICENSE.md.meta")),
    ("Package.Build.props", include_bytes!("../../../../unity/Package.Build.props")),
    ("Package.Build.props.meta", include_bytes!("../../../../unity/Package.Build.props.meta")),
    ("Package.Build.targets", include_bytes!("../../../../unity/Package.Build.targets")),
    ("Package.Build.targets.meta", include_bytes!("../../../../unity/Package.Build.targets.meta")),
    ("package.json", include_bytes!("../../../../unity/package.json")),
    ("package.json.meta", include_bytes!("../../../../unity/package.json.meta")),
    ("README.md", include_bytes!("../../../../unity/README.md")),
    ("README.md.meta", include_bytes!("../../../../unity/README.md.meta")),
    ("Tests.meta", include_bytes!("../../../../unity/Tests.meta")),
    ("Tests/Editor.meta", include_bytes!("../../../../unity/Tests/Editor.meta")),
    (
        "Tests/Editor/Prefabric.Unity.Editor.Tests.asmdef",
        include_bytes!("../../../../unity/Tests/Editor/Prefabric.Unity.Editor.Tests.asmdef"),
    ),
    (
        "Tests/Editor/Prefabric.Unity.Editor.Tests.asmdef.meta",
        include_bytes!("../../../../unity/Tests/Editor/Prefabric.Unity.Editor.Tests.asmdef.meta"),
    ),
    (
        "Tests/Editor/PrefabricInstallationTests.cs",
        include_bytes!("../../../../unity/Tests/Editor/PrefabricInstallationTests.cs"),
    ),
    (
        "Tests/Editor/PrefabricInstallationTests.cs.meta",
        include_bytes!("../../../../unity/Tests/Editor/PrefabricInstallationTests.cs.meta"),
    ),
    ("Third Party Notices.md", include_bytes!("../../../../unity/Third Party Notices.md")),
    ("Third Party Notices.md.meta", include_bytes!("../../../../unity/Third Party Notices.md.meta")),
];

/// How the project has the package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Installed {
    /// A copy in `Packages/dev.prefabric.unity`, at this version; Prefabric updates it.
    Embedded(String),
    /// Listed in `Packages/manifest.json` (git URL, `file:` path, registry): managed by hand.
    Manifest(String),
}

pub fn embedded_dir(root: &Path) -> PathBuf {
    root.join("Packages").join(NAME)
}

pub fn installed(root: &Path) -> Option<Installed> {
    let read_json = |path: PathBuf| -> Option<serde_json::Value> {
        let text = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
    };
    if let Some(package) = read_json(embedded_dir(root).join("package.json")) {
        let version = package.get("version").and_then(|v| v.as_str()).unwrap_or_default();
        return Some(Installed::Embedded(version.to_owned()));
    }
    let manifest = read_json(root.join("Packages/manifest.json"))?;
    let spec = manifest.get("dependencies")?.get(NAME)?.as_str()?;
    Some(Installed::Manifest(spec.to_owned()))
}

/// Copies the bundled package into `Packages/dev.prefabric.unity`, replacing an older copy. Unity
/// picks it up (and recompiles) the next time it refreshes. A package listed in the manifest is left
/// alone: it would conflict with the embedded copy.
pub fn install(root: &Path) -> io::Result<()> {
    if let Some(Installed::Manifest(spec)) = installed(root) {
        return Err(io::Error::other(format!("{NAME} is already listed in Packages/manifest.json ({spec})")));
    }
    if !root.join("Packages").is_dir() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Packages folder not found"));
    }
    let target = embedded_dir(root);
    // A package developed in place (a git checkout) is never replaced.
    if target.join(".git").exists() {
        return Err(io::Error::other(format!("{} is a git checkout; update it with git", target.display())));
    }
    let staging = root.join("Packages").join(format!(".{NAME}.installing"));
    let _ = std::fs::remove_dir_all(&staging);
    let written = (|| {
        for (path, bytes) in FILES {
            let file = staging.join(path);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(file, bytes)?;
        }
        io::Result::Ok(())
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    // Swap by renames, so a file held open (Unity, the language server) leaves the old package
    // whole rather than half deleted.
    let previous = root.join("Packages").join(format!(".{NAME}.previous"));
    let _ = std::fs::remove_dir_all(&previous);
    if target.exists()
        && let Err(e) = std::fs::rename(&target, &previous)
    {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&staging, &target) {
        if previous.exists() {
            let _ = std::fs::rename(&previous, &target);
        }
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    // Best effort: what can't be deleted now (a file still open) is retried on the next install.
    let _ = std::fs::remove_dir_all(&previous);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../unity")
    }

    fn files_on_disk(dir: &Path, base: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                files_on_disk(&path, base, out);
            } else if entry.file_name() != ".DS_Store" {
                out.push(path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"));
            }
        }
    }

    #[test]
    fn bundles_every_file_of_the_package_and_its_version() {
        let dir = package_dir();
        let mut on_disk = Vec::new();
        files_on_disk(&dir, &dir, &mut on_disk);
        on_disk.sort();
        let mut bundled: Vec<String> = FILES.iter().map(|(p, _)| (*p).to_owned()).collect();
        bundled.sort();
        assert_eq!(bundled, on_disk, "unity/ changed: update FILES in this file");
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("package.json")).unwrap()).unwrap();
        assert_eq!(manifest["version"], BUNDLED_VERSION);
    }

    #[test]
    fn every_asset_has_a_meta_file() {
        for (path, _) in FILES {
            if path.ends_with(".meta") || path.contains("~/") {
                continue;
            }
            let meta = format!("{path}.meta");
            assert!(FILES.iter().any(|(p, _)| *p == meta), "{path} has no .meta file");
        }
    }

    fn unity_project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Packages")).unwrap();
        std::fs::write(dir.path().join("Packages/manifest.json"), r#"{"dependencies":{"com.unity.ugui":"2.0.0"}}"#)
            .unwrap();
        dir
    }

    #[test]
    fn installs_and_updates_the_embedded_copy() {
        let dir = unity_project();
        let root = dir.path();
        assert_eq!(installed(root), None);
        install(root).unwrap();
        assert_eq!(installed(root), Some(Installed::Embedded(BUNDLED_VERSION.into())));
        assert!(embedded_dir(root).join("Editor/PrefabricEditor.cs").is_file());
        // An older copy with a stale file is replaced as a whole.
        std::fs::write(embedded_dir(root).join("package.json"), r#"{"name":"dev.prefabric.unity","version":"0.0.1"}"#)
            .unwrap();
        std::fs::write(embedded_dir(root).join("Editor/Removed.cs"), "").unwrap();
        assert_eq!(installed(root), Some(Installed::Embedded("0.0.1".into())));
        install(root).unwrap();
        assert_eq!(installed(root), Some(Installed::Embedded(BUNDLED_VERSION.into())));
        assert!(!embedded_dir(root).join("Editor/Removed.cs").exists());
        assert!(!root.join("Packages").join(format!(".{NAME}.installing")).exists());
        assert!(!root.join("Packages").join(format!(".{NAME}.previous")).exists());
        // A git checkout of the package is never replaced.
        std::fs::create_dir_all(embedded_dir(root).join(".git")).unwrap();
        std::fs::write(embedded_dir(root).join("Editor/Mine.cs"), "").unwrap();
        assert!(install(root).is_err());
        assert!(embedded_dir(root).join("Editor/Mine.cs").exists());
    }

    #[test]
    fn leaves_a_package_from_the_manifest_alone() {
        let dir = unity_project();
        let root = dir.path();
        let spec = "https://github.com/nhnpro/openrider.git?path=/unity";
        std::fs::write(root.join("Packages/manifest.json"), format!(r#"{{"dependencies":{{"{NAME}":"{spec}"}}}}"#))
            .unwrap();
        assert_eq!(installed(root), Some(Installed::Manifest(spec.into())));
        assert!(install(root).is_err());
        assert!(!embedded_dir(root).exists());
        assert!(install(&root.join("missing")).is_err());
    }
}
