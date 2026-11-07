use crate::state::AppState;
use ide_core::IdeResult;
use ide_host::ops::project::ProjectInfo;
use tauri::{State, Window};

/// Installs or updates Prefabric's Unity package in the project's `Packages/`.
#[tauri::command]
#[specta::specta]
pub async fn unity_install_package(window: Window, state: State<'_, AppState>) -> IdeResult<ProjectInfo> {
    ide_host::ops::unity::install_package(&state.host, window.label()).await
}

/// Generates the C# solution and projects with the project's Unity version in batch mode.
#[tauri::command]
#[specta::specta]
pub async fn unity_generate_project_files(window: Window, state: State<'_, AppState>) -> IdeResult<ProjectInfo> {
    ide_host::ops::unity::generate_project_files(&state.host, window.label()).await
}
