use crate::{
    AppState, AuditEventDto, AuditListRequest, CommandError, IntegrityVerificationDto,
    IntegrityVerifyRequest, commands::require_workspace,
};

pub fn audit_list(
    state: &AppState,
    request: AuditListRequest,
) -> Result<Vec<AuditEventDto>, CommandError> {
    let workspace_id = require_workspace(state, &request.workspace_id, false)?;
    state
        .core()
        .audit_events(workspace_id)
        .map(|values| values.into_iter().map(Into::into).collect())
        .map_err(Into::into)
}

pub fn integrity_verify(
    state: &AppState,
    request: IntegrityVerifyRequest,
) -> Result<IntegrityVerificationDto, CommandError> {
    let workspace_id = require_workspace(state, &request.workspace_id, false)?;
    state
        .core()
        .verify_audit(workspace_id)
        .map(Into::into)
        .map_err(Into::into)
}

pub(crate) mod tauri_handlers {
    use tauri::State;

    use super::{audit_list as list, integrity_verify as verify};
    use crate::{
        AppState, AuditEventDto, AuditListRequest, CommandError, IntegrityVerificationDto,
        IntegrityVerifyRequest, commands::run_blocking,
    };

    #[tauri::command]
    pub async fn audit_list(
        state: State<'_, AppState>,
        request: AuditListRequest,
    ) -> Result<Vec<AuditEventDto>, CommandError> {
        let state = state.inner().clone();
        run_blocking(move || list(&state, request)).await
    }

    #[tauri::command]
    pub async fn integrity_verify(
        state: State<'_, AppState>,
        request: IntegrityVerifyRequest,
    ) -> Result<IntegrityVerificationDto, CommandError> {
        let state = state.inner().clone();
        run_blocking(move || verify(&state, request)).await
    }
}
