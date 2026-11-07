//! Prefabric's Unity package in a Unity project: install or update it, and generate the C#
//! project files through Unity in batch mode.

use crate::Host;
use crate::ops::project::{ProjectInfo, info};
use ide_core::{IdeError, IdeResult};
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
pub async fn generate_project_files(host: &Arc<Host>, window: &str) -> IdeResult<ProjectInfo> {
    let project = host.project(window)?;
    project.require_trust()?;
    blocking(move || {
        let detected = unity::detect(&project.root).ok_or_else(|| IdeError::invalid("Not a Unity project"))?;
        if matches!(detected.package, Some(UnityPackageStatus::Missing)) {
            return Err(IdeError::invalid("Install Prefabric's Unity package first"));
        }
        let version = detected.editor_version.ok_or_else(|| IdeError::invalid("Unknown Unity version"))?;
        let editor = unity::editor::find(&version)
            .ok_or_else(|| IdeError::unavailable(format!("Unity {version} is not installed (Unity Hub)")))?;
        run_batch(&editor, &project.root)?;
        Ok(info(&project))
    })
    .await
}

fn run_batch(editor: &Path, root: &Path) -> IdeResult<()> {
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
        if started.elapsed() > GENERATE_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&log);
            return Err(IdeError::internal("Unity did not finish generating the project files"));
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

pub fn register(d: &mut crate::rpc::Dispatcher) {
    d.add("unity_assembly_definitions", |c| async move { assembly_definitions(&c.host, &c.window).await });
    d.add("unity_install_package", |c| async move { install_package(&c.host, &c.window).await });
    d.add("unity_generate_project_files", |c| async move { generate_project_files(&c.host, &c.window).await });
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
