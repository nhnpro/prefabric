//! Prefabric's Unity package in a Unity project: install or update it, and generate the C#
//! project files through Unity in batch mode.

use crate::Host;
use crate::ops::project::{ProjectInfo, info};
use ide_core::{CancellationToken, DocId, IdeError, IdeResult, Pos, RequestId};
use ide_unity::UnityIndexStatus;
use ide_unity::hierarchy::Hierarchy;
use ide_unity::lenses::UnityLens;
use ide_unity::search::UnitySearch;
use ide_unity::usages::{AssetUsage, symbol_at};
use ide_workspace::unity::{self, UnityPackageStatus, assemblies::AssemblyDefinition};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Importing a large project for the first time takes minutes; a hung editor is stopped.
const GENERATE_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const GENERATE_METHOD: &str = "Prefabric.Unity.Editor.ProjectFiles.Generate";

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> IdeResult<T> + Send + 'static) -> IdeResult<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| IdeError::internal(e.to_string()))?
}

/// Copies the app's package into `Packages/` (Unity reloads it when focused); also updates an older copy.
pub async fn install_package(host: &Arc<Host>, window: &str) -> IdeResult<ProjectInfo> {
    let project = host.project(window)?;
    blocking(move || {
        if !unity::is_project(&project.root) {
            return Err(IdeError::invalid("Not a Unity project"));
        }
        unity::package::install(&project.root)
            .map_err(|e| IdeError::internal(format!("Cannot install the package: {e}")))?;
        Ok(info(&project))
    })
    .await
}

/// Runs the project's Unity version in batch mode to write the solution and projects. Unity runs
/// the project's editor code, so only in a trusted project; it fails while Unity has the project open.
/// Cancellable with `cancel_request(request)`, which stops Unity.
pub async fn generate_project_files(host: &Arc<Host>, window: &str, request: RequestId) -> IdeResult<ProjectInfo> {
    let project = host.project(window)?;
    project.require_trust()?;
    let token = host.begin_project_request(window, request);
    let result = blocking(move || {
        let detected = unity::detect(&project.root).ok_or_else(|| IdeError::invalid("Not a Unity project"))?;
        if matches!(detected.package, Some(UnityPackageStatus::Missing)) {
            return Err(IdeError::invalid("Install Prefabric's Unity package first"));
        }
        let version = detected.editor_version.ok_or_else(|| IdeError::invalid("Unknown Unity version"))?;
        let editor = unity::editor::find(&version)
            .ok_or_else(|| IdeError::unavailable(format!("Unity {version} is not installed (Unity Hub)")))?;
        run_batch(&editor, &project.root, &token)?;
        Ok(info(&project))
    })
    .await;
    host.end_request(request);
    result
}

fn run_batch(editor: &Path, root: &Path, cancel: &CancellationToken) -> IdeResult<()> {
    let log = std::env::temp_dir().join(format!("prefabric-unity-{}-{}.log", std::process::id(), nonce()));
    let mut child = Command::new(editor)
        .args(["-batchmode", "-quit", "-nographics", "-projectPath"])
        .arg(root)
        .args(["-executeMethod", GENERATE_METHOD, "-logFile"])
        .arg(&log)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| IdeError::internal(format!("Cannot start Unity: {e}")))?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        let cancelled = cancel.is_cancelled();
        if cancelled || started.elapsed() > GENERATE_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&log);
            return Err(if cancelled {
                IdeError::cancelled()
            } else {
                IdeError::internal("Unity did not finish generating the project files")
            });
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    let text = std::fs::read_to_string(&log).unwrap_or_default();
    let _ = std::fs::remove_file(&log);
    if status.success() { Ok(()) } else { Err(IdeError::internal(failure_message(&text))) }
}

fn nonce() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default()
}

/// The reason in Unity's log: the project already open in Unity, or the first error lines.
fn failure_message(log: &str) -> String {
    if log.contains("another Unity instance is running")
        || log.contains("Multiple Unity instances cannot open the same project")
    {
        return "Unity has this project open: regenerate the project files from Unity, or close it and try again"
            .into();
    }
    let errors: Vec<&str> = log
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("error CS") || l.starts_with("Aborting batchmode") || l.contains("executeMethod method"))
        .take(5)
        .collect();
    if errors.is_empty() {
        "Unity could not generate the project files".into()
    } else {
        format!("Unity could not generate the project files:\n{}", errors.join("\n"))
    }
}

/// The project's assembly definitions, for navigation from `.asmdef` references.
pub async fn assembly_definitions(host: &Arc<Host>, window: &str) -> IdeResult<Vec<AssemblyDefinition>> {
    let project = host.project(window)?;
    blocking(move || Ok(unity::assemblies::assembly_definitions(&project.root))).await
}

/// State of the asset index; `None` outside Unity projects.
pub fn index_status(host: &Host, window: &str) -> IdeResult<Option<UnityIndexStatus>> {
    Ok(host.project(window)?.unity.as_ref().map(|u| u.status()))
}

/// The assets using the C# class, method or serialized field at `pos` of `doc`.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UnityAssetUsages {
    pub symbol: String,
    pub usages: Vec<AssetUsage>,
}

/// Asset usages of the symbol at `pos`, from the asset index alone (it answers in milliseconds, so
/// Find Usages shows them before the language server's references). `None` outside Unity projects,
/// C# files, and symbols assets can't use.
pub async fn asset_usages(host: &Host, window: &str, doc: DocId, pos: Pos) -> IdeResult<Option<UnityAssetUsages>> {
    let project = host.project(window)?;
    let Some(index) = project.unity.clone() else { return Ok(None) };
    blocking(move || {
        let snap = project.documents.text_snapshot(doc)?;
        if snap.large || snap.language.as_str() != "csharp" {
            return Ok(None);
        }
        let (path, _) = project.documents.identity(doc)?;
        let outline = project.syntax.outline(doc, &snap.language, snap.version, &snap.text);
        let Some(symbol) = symbol_at(&path, &snap.text, &outline, pos) else { return Ok(None) };
        Ok(Some(UnityAssetUsages { symbol: symbol.name().to_owned(), usages: index.asset_usages(&path, &symbol) }))
    })
    .await
}

/// Code Vision of a C# file: how assets use its script class, methods and serialized fields.
/// Empty outside Unity projects and C# files.
pub async fn lenses(host: &Host, window: &str, doc: DocId) -> IdeResult<Vec<UnityLens>> {
    let project = host.project(window)?;
    let Some(index) = project.unity.clone() else { return Ok(Vec::new()) };
    blocking(move || {
        let snap = project.documents.text_snapshot(doc)?;
        if snap.large || snap.language.as_str() != "csharp" {
            return Ok(Vec::new());
        }
        let (path, _) = project.documents.identity(doc)?;
        let outline = project.syntax.outline(doc, &snap.language, snap.version, &snap.text);
        Ok(index.lenses(&path, &outline))
    })
    .await
}

/// Search Everywhere's Unity tab: GameObjects, prefab instances and assets matching `query`.
pub async fn search(host: &Host, window: &str, query: String, limit: u32) -> IdeResult<UnitySearch> {
    let project = host.project(window)?;
    let Some(index) = project.unity.clone() else { return Ok(UnitySearch::default()) };
    blocking(move || Ok(index.search(&query, limit.min(1_000) as usize))).await
}

/// The GameObject tree of a scene or prefab of the project, as saved.
pub async fn hierarchy(host: &Host, window: &str, path: ide_core::IpcPath) -> IdeResult<Hierarchy> {
    let project = host.project(window)?;
    let Some(index) = project.unity.clone() else { return Ok(Hierarchy { roots: Vec::new(), unreadable: true }) };
    let path = std::path::PathBuf::from(path);
    if !path.starts_with(index.root()) {
        return Err(IdeError::invalid(format!("{} is outside the Unity project", path.display())));
    }
    blocking(move || ide_unity::hierarchy::read(&index, &path).map_err(|e| IdeError::internal(e.to_string()))).await
}

pub fn register(d: &mut crate::rpc::Dispatcher) {
    d.add("unity_hierarchy", |c| async move { hierarchy(&c.host, &c.window, c.arg("path")?).await });
    d.add("unity_search", |c| async move { search(&c.host, &c.window, c.arg("query")?, c.arg("limit")?).await });
    d.add("unity_lenses", |c| async move { lenses(&c.host, &c.window, c.arg("doc")?).await });
    d.add(
        "unity_asset_usages",
        |c| async move { asset_usages(&c.host, &c.window, c.arg("doc")?, c.arg("pos")?).await },
    );
    d.add("unity_index_status", |c| async move { index_status(&c.host, &c.window) });
    d.add("unity_assembly_definitions", |c| async move { assembly_definitions(&c.host, &c.window).await });
    d.add("unity_install_package", |c| async move { install_package(&c.host, &c.window).await });
    d.add("unity_generate_project_files", |c| async move {
        generate_project_files(&c.host, &c.window, c.arg("request")?).await
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explains_why_generation_failed() {
        assert!(failure_message("Aborting batchmode due to failure:\nIt looks like another Unity instance is running with this project open.").contains("Unity has this project open"));
        let log =
            "Loading\nAssets/Scripts/Player.cs(3,1): error CS1002: ; expected\nAborting batchmode due to failure:\n";
        assert_eq!(
            failure_message(log),
            "Unity could not generate the project files:\nAssets/Scripts/Player.cs(3,1): error CS1002: ; expected\nAborting batchmode due to failure:"
        );
        assert_eq!(failure_message(""), "Unity could not generate the project files");
    }
}
