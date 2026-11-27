//! The Roslyn language server (`Microsoft.CodeAnalysis.LanguageServer`, the C#
//! server of VS Code's C# extension). It loads nothing until the client names a
//! solution (`solution/open`) or projects (`project/open`), reports the end of
//! loading with `workspace/projectInitializationComplete`, and asks the client
//! to restore NuGet packages with `workspace/_roslyn_projectNeedsRestore`.

use crate::LspManager;
use crate::server::{Running, Server};
use crate::uri;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) const SERVER: &str = "roslyn";
pub(crate) const PROJECTS_LOADED: &str = "workspace/projectInitializationComplete";
pub(crate) const NEEDS_RESTORE: &str = "workspace/_roslyn_projectNeedsRestore";
const RESTORE: &str = "workspace/_roslyn_restore";

/// Folders never searched for projects: build output, Unity's generated
/// folders, dependencies.
const SKIPPED: [&str; 7] = ["bin", "obj", "node_modules", "Library", "Temp", "Logs", "packages"];
const PROJECT_DEPTH: usize = 4;
const PROJECT_LIMIT: usize = 500;

#[derive(Debug, PartialEq)]
pub(crate) enum Target {
    Solution(PathBuf),
    Projects(Vec<PathBuf>),
}

/// What Roslyn loads for a workspace root: a solution in the root (the one named
/// like the folder first, `.slnx` before `.sln`; Unity names its solution after
/// the project folder), or else the C# projects below the root.
pub(crate) fn target(root: &Path) -> Option<Target> {
    let folder = root.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_owned();
    let mut solutions: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && matches!(extension(p), "slnx" | "sln"))
        .collect();
    solutions.sort_by_key(|p| {
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
        (stem != folder, extension(p) != "slnx", stem)
    });
    if let Some(solution) = solutions.into_iter().next() {
        return Some(Target::Solution(solution));
    }
    let mut projects = Vec::new();
    find_projects(root, 0, &mut projects);
    projects.sort();
    (!projects.is_empty()).then_some(Target::Projects(projects))
}

fn extension(path: &Path) -> &str {
    path.extension().and_then(|e| e.to_str()).unwrap_or_default()
}

fn find_projects(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if out.len() >= PROJECT_LIMIT {
            return;
        }
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if depth < PROJECT_DEPTH && !name.starts_with('.') && !SKIPPED.contains(&name.as_ref()) {
                find_projects(&path, depth + 1, out);
            }
        } else if extension(&path) == "csproj" {
            out.push(path);
        }
    }
}

fn notification(target: &Target) -> (&'static str, Value) {
    match target {
        Target::Solution(path) => ("solution/open", json!({ "solution": uri::path_to_uri_string(path) })),
        Target::Projects(paths) => {
            let uris: Vec<_> = paths.iter().map(|p| uri::path_to_uri_string(p)).collect();
            ("project/open", json!({ "projects": uris }))
        }
    }
}

/// Roslyn writes decompiled and source-link sources of referenced assemblies
/// (Go to Definition on `List<T>`) under `<temp>/MetadataAsSource/`. They are
/// regenerated on demand, so edits would be lost: make them read-only, which
/// opens them read-only in the editor.
pub(crate) fn protect_metadata_sources(paths: &[PathBuf]) {
    let generated = std::env::temp_dir().join("MetadataAsSource");
    let generated = dunce::canonicalize(&generated).unwrap_or(generated);
    for path in paths {
        let canonical = dunce::canonicalize(path).unwrap_or_else(|_| path.clone());
        if !canonical.starts_with(&generated) {
            continue;
        }
        if let Ok(meta) = std::fs::metadata(&canonical) {
            let mut permissions = meta.permissions();
            if !permissions.readonly() {
                permissions.set_readonly(true);
                let _ = std::fs::set_permissions(&canonical, permissions);
            }
        }
    }
}

impl LspManager {
    /// Opens the root's solution or projects in a just-initialized Roslyn
    /// instance; the server counts as loading until it reports [`PROJECTS_LOADED`].
    pub(crate) async fn open_roslyn_projects(&self, server: &Server, run: &Running) {
        let root = server.root.clone();
        let Ok(Some(target)) = tokio::task::spawn_blocking(move || target(&root)).await else {
            tracing::info!("{}: no solution or project under {}", server.name, server.root.display());
            return;
        };
        let (method, params) = notification(&target);
        tracing::info!("{}: {method} {target:?}", server.name);
        server.set_loading(self.sink(), true);
        if let Err(e) = run.peer.notify(method, params) {
            tracing::warn!("{}: {method}: {e}", server.name);
            server.set_loading(self.sink(), false);
        }
    }

    /// Answers [`NEEDS_RESTORE`] by asking the server to restore those projects,
    /// as VS Code's C# extension does. The server runs `dotnet restore`; it only
    /// runs at all in a trusted project.
    pub(crate) fn restore_roslyn_projects(&self, server: Arc<Server>, generation: u64, params: Value) {
        let projects = params.get("projectFilePaths").cloned().unwrap_or_else(|| json!([]));
        self.runtime.spawn(async move {
            let Some(run) = server.initialized_gen(generation) else { return };
            let restore = run.peer.request::<_, Value>(RESTORE, json!({ "projectFilePaths": projects }), None);
            if let Err(e) = restore.await {
                tracing::warn!("{}: {RESTORE}: {e}", server.name);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "").unwrap();
    }

    #[test]
    fn prefers_the_solution_named_like_the_folder_and_slnx() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Game");
        for name in ["Aaa.sln", "Game.sln", "Game.slnx", "Zzz.slnx"] {
            touch(&root.join(name));
        }
        assert_eq!(target(&root), Some(Target::Solution(root.join("Game.slnx"))));
        std::fs::remove_file(root.join("Game.slnx")).unwrap();
        assert_eq!(target(&root), Some(Target::Solution(root.join("Game.sln"))));
        std::fs::remove_file(root.join("Game.sln")).unwrap();
        // No solution named like the folder: `.slnx` first, then by name.
        assert_eq!(target(&root), Some(Target::Solution(root.join("Zzz.slnx"))));
    }

    #[test]
    fn without_a_solution_opens_the_projects_outside_generated_folders() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(&root.join("src/App/App.csproj"));
        touch(&root.join("tests/App.Tests/App.Tests.csproj"));
        touch(&root.join("src/App/obj/Generated.csproj"));
        touch(&root.join("Library/PackageCache/pkg/Pkg.csproj"));
        touch(&root.join(".git/x.csproj"));
        touch(&root.join("a/b/c/d/e/TooDeep.csproj"));
        assert_eq!(
            target(root),
            Some(Target::Projects(vec![
                root.join("src/App/App.csproj"),
                root.join("tests/App.Tests/App.Tests.csproj")
            ]))
        );
        assert_eq!(
            target(&root.join("src/App/obj")),
            Some(Target::Projects(vec![root.join("src/App/obj/Generated.csproj")]))
        );
        assert_eq!(target(&root.join("docs")), None);
    }

    #[test]
    fn only_generated_metadata_sources_become_read_only() {
        let generated = std::env::temp_dir().join("MetadataAsSource").join(format!("test-{}", std::process::id()));
        std::fs::create_dir_all(&generated).unwrap();
        let decompiled = generated.join("List.cs");
        std::fs::write(&decompiled, "class List {}").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("Program.cs");
        std::fs::write(&project, "").unwrap();
        protect_metadata_sources(&[decompiled.clone(), project.clone()]);
        assert!(std::fs::metadata(&decompiled).unwrap().permissions().readonly());
        assert!(!std::fs::metadata(&project).unwrap().permissions().readonly());
        let mut writable = std::fs::metadata(&decompiled).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        writable.set_readonly(false);
        std::fs::set_permissions(&decompiled, writable).unwrap();
        std::fs::remove_dir_all(&generated).unwrap();
    }

    #[test]
    fn notifications_carry_file_uris() {
        let (method, params) = notification(&Target::Solution(PathBuf::from("/w/Game.sln")));
        assert_eq!(method, "solution/open");
        assert!(params["solution"].as_str().unwrap().starts_with("file://"));
        let (method, params) = notification(&Target::Projects(vec![PathBuf::from("/w/A.csproj")]));
        assert_eq!(method, "project/open");
        assert_eq!(params["projects"].as_array().unwrap().len(), 1);
    }
}
