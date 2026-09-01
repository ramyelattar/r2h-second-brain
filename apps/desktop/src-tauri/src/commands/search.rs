use knowledge_domain::{SearchHit, SearchRequest, WorkspaceId};

use crate::{
    AppState, CitationDto, CitationResolveRequest, CommandError, SearchHitDto, SearchRequestDto,
    commands::require_workspace,
};

pub fn search_execute(
    state: &AppState,
    request: SearchRequestDto,
) -> Result<Vec<SearchHitDto>, CommandError> {
    let workspace_id = require_workspace(state, &request.workspace_id, true)?;
    let search_request = SearchRequest::new(
        workspace_id,
        request.query,
        request.source_kinds.into_iter().map(Into::into).collect(),
        request.limit,
    )?;
    state
        .core()
        .search(&search_request)
        .map(|values| values.into_iter().map(Into::into).collect())
        .map_err(Into::into)
}

pub fn citation_resolve(
    state: &AppState,
    request: CitationResolveRequest,
) -> Result<CitationDto, CommandError> {
    let workspace_id: WorkspaceId = require_workspace(state, &request.workspace_id, false)?;
    let hit: SearchHit = request.hit.try_into()?;
    state
        .core()
        .resolve_citation(workspace_id, &hit)
        .map(Into::into)
        .map_err(Into::into)
}

pub(crate) mod tauri_handlers {
    use tauri::State;

    use super::{citation_resolve as resolve, search_execute as search};
    use crate::{
        AppState, CitationDto, CitationResolveRequest, CommandError, SearchHitDto,
        SearchRequestDto, commands::run_blocking,
    };

    #[tauri::command]
    pub async fn search_execute(
        state: State<'_, AppState>,
        request: SearchRequestDto,
    ) -> Result<Vec<SearchHitDto>, CommandError> {
        let state = state.inner().clone();
        run_blocking(move || search(&state, request)).await
    }

    #[tauri::command]
    pub async fn citation_resolve(
        state: State<'_, AppState>,
        request: CitationResolveRequest,
    ) -> Result<CitationDto, CommandError> {
        let state = state.inner().clone();
        run_blocking(move || resolve(&state, request)).await
    }
}
