use crate::{
    catalog::Catalog, contracts::*, git::GitEngine, launch::LaunchQueue, operations::Operations,
};
use std::path::Path;
use tauri::State;

pub struct AppState {
    pub engine: GitEngine,
    pub catalog: Catalog,
    pub operations: Operations,
    pub launches: LaunchQueue,
    #[cfg(feature = "native-smoke")]
    pub smoke: Option<crate::native_smoke::Fixture>,
}
#[tauri::command]
pub async fn open_repository(
    state: State<'_, AppState>,
    path: String,
) -> AppResult<RepositoryOpen> {
    state.engine.open_repository(Path::new(&path)).await
}
#[tauri::command]
pub async fn open_project(state: State<'_, AppState>, path: String) -> AppResult<Project> {
    state.catalog.open(&state.engine, Path::new(&path)).await
}
#[tauri::command]
pub async fn refresh_project(state: State<'_, AppState>, project_id: String) -> AppResult<Project> {
    state.catalog.refresh(&state.engine, &project_id).await
}
#[tauri::command]
pub async fn create_project(
    state: State<'_, AppState>,
    path: String,
    content: ProjectContent,
) -> AppResult<Project> {
    state
        .catalog
        .create(&state.engine, Path::new(&path), content)
        .await
}
#[tauri::command]
pub async fn save_project(
    state: State<'_, AppState>,
    project_id: String,
    expected_revision: String,
    content: ProjectContent,
) -> AppResult<Project> {
    state
        .catalog
        .save(&state.engine, &project_id, &expected_revision, content)
        .await
}
#[tauri::command]
pub async fn get_status(
    state: State<'_, AppState>,
    repository_id: String,
) -> AppResult<RepositoryStatus> {
    state.engine.get_status(&repository_id).await
}
#[tauri::command]
pub async fn get_changes(state: State<'_, AppState>, repository_id: String) -> AppResult<Changes> {
    state.engine.get_changes(&repository_id).await
}
#[tauri::command]
pub async fn list_refs(
    state: State<'_, AppState>,
    repository_id: String,
) -> AppResult<Vec<GitRef>> {
    state.engine.list_refs(&repository_id).await
}
#[tauri::command]
pub async fn list_remotes(
    state: State<'_, AppState>,
    repository_id: String,
) -> AppResult<Vec<Remote>> {
    state.engine.list_remotes(&repository_id).await
}
#[tauri::command]
pub async fn get_history(
    state: State<'_, AppState>,
    repository_id: String,
    scope: Option<String>,
    snapshot_id: Option<String>,
    cursor: Option<String>,
) -> AppResult<HistoryPage> {
    state
        .engine
        .get_history(&repository_id, scope, snapshot_id, cursor)
        .await
}
#[tauri::command]
pub async fn get_commit(
    state: State<'_, AppState>,
    repository_id: String,
    oid: String,
    parent_oid: Option<String>,
) -> AppResult<CommitDetails> {
    state
        .engine
        .get_commit(&repository_id, &oid, parent_oid)
        .await
}
#[tauri::command]
pub async fn get_diff(state: State<'_, AppState>, request: DiffRequest) -> AppResult<DiffResult> {
    state.engine.get_diff(request).await
}
#[tauri::command]
pub async fn plan_repository_action(
    state: State<'_, AppState>,
    repository_id: String,
    action: RepositoryAction,
) -> AppResult<RepositoryActionPlan> {
    Ok(state.operations.plan_action(&repository_id, action).await)
}
#[tauri::command]
pub async fn run_repository_action(
    state: State<'_, AppState>,
    plan_id: String,
) -> AppResult<OperationResult> {
    state.operations.run_action(&plan_id).await
}
#[tauri::command]
pub async fn plan_batch(state: State<'_, AppState>, request: BatchRequest) -> AppResult<BatchPlan> {
    state.operations.plan_batch(request).await
}
#[tauri::command]
pub async fn run_batch(state: State<'_, AppState>, plan_id: String) -> AppResult<Batch> {
    state.operations.run_batch(&plan_id).await
}
#[tauri::command]
pub async fn cancel_batch(state: State<'_, AppState>, batch_id: String) -> AppResult<()> {
    state.operations.cancel(&batch_id).await
}
#[tauri::command]
pub async fn get_operations(
    state: State<'_, AppState>,
    batch_id: Option<String>,
) -> AppResult<Vec<OperationResult>> {
    Ok(state.operations.get(batch_id.as_deref()).await)
}
#[tauri::command]
pub fn get_launch_request(state: State<'_, AppState>) -> Option<LaunchRequest> {
    state.launches.next()
}
#[tauri::command]
pub fn ack_launch_request(state: State<'_, AppState>, request_id: String) {
    state.launches.ack(&request_id)
}

#[tauri::command]
pub async fn list_rewrites(
    state: State<'_, AppState>,
    repository_id: String,
) -> AppResult<Vec<RewriteRecord>> {
    state.engine.list_rewrites(&repository_id).await
}
