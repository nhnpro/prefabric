use crate::state::AppState;
use ide_core::{DocId, IdeResult, Pos, RequestId};
use ide_host::ops::project::ProjectInfo;
use ide_host::ops::unity::UnityAssetUsages;
use ide_unity::UnityIndexStatus;
use ide_workspace::unity::assemblies::AssemblyDefinition;
use tauri::{State, Window};

/// Installs or updates Prefabric's Unity package in the project's `Packages/`.
#[tauri::command]
#[specta::specta]
pub async fn unity_install_package(window: Window, state: State<'_, AppState>) -> IdeResult<ProjectInfo> {
    ide_host::ops::unity::install_package(&state.host, window.label()).await
}

/// Generates the C# solution and projects with the project's Unity version in batch mode;
/// `cancel_request(request)` stops Unity.
#[tauri::command]
#[specta::specta]
pub async fn unity_generate_project_files(
    window: Window,
    state: State<'_, AppState>,
    request: RequestId,
) -> IdeResult<ProjectInfo> {
    ide_host::ops::unity::generate_project_files(&state.host, window.label(), request).await
}

/// The project's assembly definitions (`.asmdef`), with their GUIDs.
#[tauri::command]
#[specta::specta]
pub async fn unity_assembly_definitions(
    window: Window,
    state: State<'_, AppState>,
) -> IdeResult<Vec<AssemblyDefinition>> {
    ide_host::ops::unity::assembly_definitions(&state.host, window.label()).await
}

/// State of the Unity asset index; `null` outside Unity projects.
#[tauri::command]
#[specta::specta]
pub fn unity_index_status(window: Window, state: State<'_, AppState>) -> IdeResult<Option<UnityIndexStatus>> {
    ide_host::ops::unity::index_status(&state.host, window.label())
}

/// The scenes, prefabs and clips using the C# class, method or serialized field at `pos`;
/// `null` outside Unity projects and for symbols assets can't use.
#[tauri::command]
#[specta::specta]
pub async fn unity_asset_usages(
    window: Window,
    state: State<'_, AppState>,
    doc: DocId,
    pos: Pos,
) -> IdeResult<Option<UnityAssetUsages>> {
    ide_host::ops::unity::asset_usages(&state.host, window.label(), doc, pos).await
}
