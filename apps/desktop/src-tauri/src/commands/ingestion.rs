use std::path::PathBuf;

use knowledge_domain::SourceId;

use crate::{
    AppState, CommandError, IngestFileResultDto, IngestFilesRequest, IngestStatusDto, SourceDto,
    SourceGetRequest, SourceListRequest, commands::require_workspace, parse_id,
};

const MAXIMUM_FILE_COUNT: usize = 100;

pub fn source_ingest_files(
    state: &AppState,
    request: IngestFilesRequest,
) -> Result<Vec<IngestFileResultDto>, CommandError> {
    let workspace_id = require_workspace(state, &request.workspace_id, true)?;
    if request.paths.is_empty() || request.paths.len() > MAXIMUM_FILE_COUNT {
        return Err(CommandError::InvalidRequest(
            "file count must be between 1 and 100".to_owned(),
        ));
    }
    Ok(request
        .paths
        .into_iter()
        .map(|path| {
            if path.trim().is_empty() {
                return IngestFileResultDto {
                    path,
                    status: IngestStatusDto::Failed,
                    source_id: None,
                    document_version_id: None,
                    error: Some(CommandError::InvalidRequest(
                        "source path is blank".to_owned(),
                    )),
                };
            }
            match state
                .core()
                .import_and_index(workspace_id, &PathBuf::from(&path))
            {
                Ok(document) => IngestFileResultDto {
                    path,
                    status: IngestStatusDto::Succeeded,
                    source_id: Some(document.outcome.source_id.to_string()),
                    document_version_id: Some(document.outcome.document_version_id.to_string()),
                    error: None,
                },
                Err(error) => IngestFileResultDto {
                    path,
                    status: IngestStatusDto::Failed,
                    source_id: None,
                    document_version_id: None,
                    error: Some(error.into()),
                },
            }
        })
        .collect())
}

pub fn source_list(
    state: &AppState,
    request: SourceListRequest,
) -> Result<Vec<SourceDto>, CommandError> {
    let workspace_id = require_workspace(state, &request.workspace_id, true)?;
    state
        .core()
        .sources(workspace_id)?
        .into_iter()
        .map(|source| {
            let versions = state.core().document_versions(workspace_id, source.id)?;
            Ok(SourceDto::new(source, versions))
        })
        .collect()
}

pub fn source_get(state: &AppState, request: SourceGetRequest) -> Result<SourceDto, CommandError> {
    let workspace_id = require_workspace(state, &request.workspace_id, true)?;
    let source_id: SourceId = parse_id(&request.source_id, "source ID")?;
    let source = state
        .core()
        .source(workspace_id, source_id)?
        .ok_or(CommandError::NotFound)?;
    let versions = state.core().document_versions(workspace_id, source_id)?;
    Ok(SourceDto::new(source, versions))
}

pub(crate) mod tauri_handlers {
    use tauri::State;

    use super::{source_get as get, source_ingest_files as ingest, source_list as list};
    use crate::{
        AppState, CommandError, IngestFileResultDto, IngestFilesRequest, SourceDto,
        SourceGetRequest, SourceListRequest, commands::run_blocking,
    };

    #[tauri::command]
    pub async fn source_ingest_files(
        state: State<'_, AppState>,
        request: IngestFilesRequest,
    ) -> Result<Vec<IngestFileResultDto>, CommandError> {
        let state = state.inner().clone();
        run_blocking(move || ingest(&state, request)).await
    }

    #[tauri::command]
    pub async fn source_list(
        state: State<'_, AppState>,
        request: SourceListRequest,
    ) -> Result<Vec<SourceDto>, CommandError> {
        let state = state.inner().clone();
        run_blocking(move || list(&state, request)).await
    }

    #[tauri::command]
    pub async fn source_get(
        state: State<'_, AppState>,
        request: SourceGetRequest,
    ) -> Result<SourceDto, CommandError> {
        let state = state.inner().clone();
        run_blocking(move || get(&state, request)).await
    }
}
