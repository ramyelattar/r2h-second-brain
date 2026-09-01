use std::path::PathBuf;

use crate::{
    AppState, BackupManifestDto, CommandError, CreateBackupRequest, OrphanBlobsDto,
    RestoreReportDto, RestoreWorkspaceBackupRequest, ScanOrphanBlobsRequest,
    VerifyWorkspaceIntegrityRequest, WorkspaceIntegrityDto, commands::require_workspace, parse_id,
};
use knowledge_domain::WorkspaceId;

pub fn create_consistent_backup(
    state: &AppState,
    request: CreateBackupRequest,
) -> Result<BackupManifestDto, CommandError> {
    let workspace_id = require_workspace(state, &request.workspace_id, false)?;
    state
        .core()
        .create_consistent_backup(workspace_id, &PathBuf::from(request.destination))
        .map(Into::into)
        .map_err(Into::into)
}

pub fn verify_workspace_integrity(
    state: &AppState,
    request: VerifyWorkspaceIntegrityRequest,
) -> Result<WorkspaceIntegrityDto, CommandError> {
    let workspace_id = require_workspace(state, &request.workspace_id, false)?;
    state
        .core()
        .verify_workspace_integrity(workspace_id, request.mode.into())
        .map(Into::into)
        .map_err(Into::into)
}

pub fn scan_orphan_blobs(
    state: &AppState,
    request: ScanOrphanBlobsRequest,
) -> Result<OrphanBlobsDto, CommandError> {
    let workspace_id = require_workspace(state, &request.workspace_id, false)?;
    state
        .core()
        .scan_orphan_blobs(workspace_id)
        .map(Into::into)
        .map_err(Into::into)
}

pub fn restore_workspace_backup(
    state: &AppState,
    request: RestoreWorkspaceBackupRequest,
) -> Result<RestoreReportDto, CommandError> {
    let new_workspace_id: WorkspaceId = parse_id(&request.new_workspace_id, "workspace ID")?;
    state
        .core()
        .restore_workspace_backup(&PathBuf::from(request.backup), new_workspace_id)
        .map(Into::into)
        .map_err(Into::into)
}

pub(crate) mod tauri_handlers {
    use tauri::State;

    use super::{
        create_consistent_backup as create, restore_workspace_backup as restore,
        scan_orphan_blobs as scan, verify_workspace_integrity as verify,
    };
    use crate::{
        AppState, BackupManifestDto, CommandError, CreateBackupRequest, OrphanBlobsDto,
        RestoreReportDto, RestoreWorkspaceBackupRequest, ScanOrphanBlobsRequest,
        VerifyWorkspaceIntegrityRequest, WorkspaceIntegrityDto, commands::run_blocking,
    };

    #[tauri::command]
    pub async fn create_consistent_backup(
        state: State<'_, AppState>,
        request: CreateBackupRequest,
    ) -> Result<BackupManifestDto, CommandError> {
        let state = state.inner().clone();
        run_blocking(move || create(&state, request)).await
    }

    #[tauri::command]
    pub async fn verify_workspace_integrity(
        state: State<'_, AppState>,
        request: VerifyWorkspaceIntegrityRequest,
    ) -> Result<WorkspaceIntegrityDto, CommandError> {
        let state = state.inner().clone();
        run_blocking(move || verify(&state, request)).await
    }

    #[tauri::command]
    pub async fn scan_orphan_blobs(
        state: State<'_, AppState>,
        request: ScanOrphanBlobsRequest,
    ) -> Result<OrphanBlobsDto, CommandError> {
        let state = state.inner().clone();
        run_blocking(move || scan(&state, request)).await
    }

    #[tauri::command]
    pub async fn restore_workspace_backup(
        state: State<'_, AppState>,
        request: RestoreWorkspaceBackupRequest,
    ) -> Result<RestoreReportDto, CommandError> {
        let state = state.inner().clone();
        run_blocking(move || restore(&state, request)).await
    }
}
