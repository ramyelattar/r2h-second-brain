pub(crate) mod audit;
pub(crate) mod ingestion;
pub(crate) mod maintenance;
pub(crate) mod search;
pub(crate) mod workspaces;

use knowledge_domain::WorkspaceId;

use crate::{AppState, CommandError, parse_id};

pub use audit::{audit_list, integrity_verify};
pub use ingestion::{source_get, source_ingest_files, source_list};
pub use maintenance::{
    create_consistent_backup, restore_workspace_backup, scan_orphan_blobs,
    verify_workspace_integrity,
};
pub use search::{citation_resolve, search_execute};
pub use workspaces::{workspace_create, workspace_list};

pub(crate) fn require_workspace(
    state: &AppState,
    value: &str,
    require_active: bool,
) -> Result<WorkspaceId, CommandError> {
    let workspace_id = parse_id(value, "workspace ID")?;
    let workspace = state
        .core()
        .workspace(workspace_id)?
        .ok_or(CommandError::NotFound)?;
    if require_active && workspace.archived_at_utc.is_some() {
        return Err(CommandError::AccessDenied);
    }
    Ok(workspace.id)
}

pub(crate) async fn run_blocking<T, F>(operation: F) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CommandError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| CommandError::StorageFailure("command execution failed".to_owned()))?
}
