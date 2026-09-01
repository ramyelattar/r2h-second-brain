use crate::{
    AppState, CommandError, CreateWorkspaceRequest, WorkspaceDto, validate_workspace_name,
};

pub fn workspace_create(
    state: &AppState,
    request: CreateWorkspaceRequest,
) -> Result<WorkspaceDto, CommandError> {
    validate_workspace_name(&request.name)?;
    state
        .core()
        .create_workspace(&request.name)
        .map(Into::into)
        .map_err(Into::into)
}

pub fn workspace_list(state: &AppState) -> Result<Vec<WorkspaceDto>, CommandError> {
    state
        .core()
        .list_workspaces()
        .map(|values| values.into_iter().map(Into::into).collect())
        .map_err(Into::into)
}

pub(crate) mod tauri_handlers {
    use tauri::State;

    use super::{workspace_create as create, workspace_list as list};
    use crate::{
        AppState, CommandError, CreateWorkspaceRequest, WorkspaceDto, commands::run_blocking,
    };

    #[tauri::command]
    pub async fn workspace_create(
        state: State<'_, AppState>,
        request: CreateWorkspaceRequest,
    ) -> Result<WorkspaceDto, CommandError> {
        let state = state.inner().clone();
        run_blocking(move || create(&state, request)).await
    }

    #[tauri::command]
    pub async fn workspace_list(
        state: State<'_, AppState>,
    ) -> Result<Vec<WorkspaceDto>, CommandError> {
        let state = state.inner().clone();
        run_blocking(move || list(&state)).await
    }
}
