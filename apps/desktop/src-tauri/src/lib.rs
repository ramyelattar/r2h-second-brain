#![forbid(unsafe_code)]

//! Secure desktop command boundary for the local Knowledge Core.

mod ai_pack;
mod app_state;
mod chat_bridge;
mod commands;
mod dto;
mod embedding_client;
mod embedding_worker;
mod khoj_runtime;
mod local_model_runtime;
mod prime_integration;
mod rag;
mod reranker_client;
mod retrieval_runtime;

#[cfg(test)]
mod live_probe;

use std::{
    env,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    thread,
};

use knowledge_app::AppConfig;
use tauri::{Emitter, Manager};

use crate::ai_pack::R2hAiPackResolver;
use crate::prime_integration::PrimeCapabilityBroker;

pub use app_state::AppState;
pub use commands::{
    audit_list, citation_resolve, create_consistent_backup, integrity_verify,
    restore_workspace_backup, scan_orphan_blobs, search_execute, source_get, source_ingest_files,
    source_list, verify_workspace_integrity, workspace_create, workspace_list,
};
pub use dto::*;
pub use embedding_worker::{EmbeddingWorkerManager, EmbeddingWorkerPass};
pub use khoj_runtime::{
    KhojProcess, KhojProcessLauncher, KhojReadinessProbe, KhojRuntimeConfig, KhojRuntimeManager,
    KhojRuntimeState, KhojRuntimeStatus,
};
pub use local_model_runtime::{
    LocalModelRuntimeConfig, LocalModelRuntimeManager, LocalModelRuntimeState,
    LocalModelRuntimeStatus,
};
pub use reranker_client::RerankerClient;
pub use retrieval_runtime::{
    RetrievalRuntimeConfig, RetrievalRuntimeManager, RetrievalRuntimeRole, RetrievalRuntimeState,
    RetrievalRuntimeStatus,
};

#[derive(Clone)]
struct RetrievalRuntimeRegistry {
    embedding: RetrievalRuntimeManager,
    reranker: RetrievalRuntimeManager,
}

impl RetrievalRuntimeRegistry {
    fn development_with_pack(project_root: PathBuf, pack: R2hAiPackResolver) -> Self {
        Self {
            embedding: RetrievalRuntimeManager::new(RetrievalRuntimeConfig::embedding_with_pack(
                project_root.clone(),
                pack.clone(),
            )),
            reranker: RetrievalRuntimeManager::new(RetrievalRuntimeConfig::reranker_with_pack(
                project_root,
                pack,
            )),
        }
    }
}

static SHUTDOWN_STARTED: AtomicBool = AtomicBool::new(false);

pub const COMMAND_NAMES: [&str; 34] = [
    "workspace_create",
    "workspace_list",
    "source_ingest_files",
    "source_list",
    "source_get",
    "search_execute",
    "citation_resolve",
    "audit_list",
    "integrity_verify",
    "create_consistent_backup",
    "verify_workspace_integrity",
    "scan_orphan_blobs",
    "restore_workspace_backup",
    "khoj_runtime_status",
    "khoj_runtime_start",
    "khoj_runtime_stop",
    "khoj_runtime_restart",
    "local_model_runtime_status",
    "local_model_runtime_start",
    "local_model_runtime_stop",
    "local_model_runtime_restart",
    "embedding_runtime_status",
    "embedding_runtime_start",
    "embedding_runtime_stop",
    "embedding_runtime_restart",
    "reranker_runtime_status",
    "reranker_runtime_start",
    "reranker_runtime_stop",
    "reranker_runtime_restart",
    "chat_send",
    "chat_sessions",
    "chat_history",
    "chat_stream_start",
    "chat_stream_cancel",
];

fn runtime_status_result(runtime: &KhojRuntimeManager) -> Result<KhojRuntimeStatus, String> {
    runtime
        .status()
        .map_err(|_| "Khoj runtime status is unavailable".to_owned())
}

#[tauri::command]
fn khoj_runtime_status(
    runtime: tauri::State<'_, KhojRuntimeManager>,
) -> Result<KhojRuntimeStatus, String> {
    runtime_status_result(runtime.inner())
}

#[tauri::command]
fn khoj_runtime_start(
    runtime: tauri::State<'_, KhojRuntimeManager>,
) -> Result<KhojRuntimeStatus, String> {
    let owned_runtime = runtime.inner().clone();
    thread::spawn(move || {
        let _ = owned_runtime.start();
    });

    runtime_status_result(runtime.inner())
}

#[tauri::command]
fn khoj_runtime_stop(
    runtime: tauri::State<'_, KhojRuntimeManager>,
) -> Result<KhojRuntimeStatus, String> {
    let owned_runtime = runtime.inner().clone();
    thread::spawn(move || {
        let _ = owned_runtime.stop();
    });

    runtime_status_result(runtime.inner())
}

#[tauri::command]
fn khoj_runtime_restart(
    runtime: tauri::State<'_, KhojRuntimeManager>,
) -> Result<KhojRuntimeStatus, String> {
    let owned_runtime = runtime.inner().clone();
    thread::spawn(move || {
        if owned_runtime.stop().is_ok() {
            let _ = owned_runtime.start();
        }
    });

    runtime_status_result(runtime.inner())
}

fn local_model_status_result(
    runtime: &LocalModelRuntimeManager,
) -> Result<LocalModelRuntimeStatus, String> {
    runtime
        .status()
        .map_err(|_| "Local Qwen runtime status is unavailable".to_owned())
}

#[tauri::command]
fn local_model_runtime_status(
    runtime: tauri::State<'_, LocalModelRuntimeManager>,
) -> Result<LocalModelRuntimeStatus, String> {
    local_model_status_result(runtime.inner())
}

#[tauri::command]
fn local_model_runtime_start(
    runtime: tauri::State<'_, LocalModelRuntimeManager>,
) -> Result<LocalModelRuntimeStatus, String> {
    let owned_runtime = runtime.inner().clone();

    thread::spawn(move || {
        let _ = owned_runtime.start();
    });

    local_model_status_result(runtime.inner())
}

#[tauri::command]
fn local_model_runtime_stop(
    runtime: tauri::State<'_, LocalModelRuntimeManager>,
) -> Result<LocalModelRuntimeStatus, String> {
    let owned_runtime = runtime.inner().clone();

    thread::spawn(move || {
        let _ = owned_runtime.stop();
    });

    local_model_status_result(runtime.inner())
}

#[tauri::command]
fn local_model_runtime_restart(
    runtime: tauri::State<'_, LocalModelRuntimeManager>,
) -> Result<LocalModelRuntimeStatus, String> {
    let owned_runtime = runtime.inner().clone();

    thread::spawn(move || {
        if owned_runtime.stop().is_ok() {
            let _ = owned_runtime.start();
        }
    });

    local_model_status_result(runtime.inner())
}

fn retrieval_status_result(
    runtime: &RetrievalRuntimeManager,
    unavailable_message: &str,
) -> Result<RetrievalRuntimeStatus, String> {
    runtime.status().map_err(|_| unavailable_message.to_owned())
}

fn start_retrieval_runtime(runtime: RetrievalRuntimeManager) {
    thread::spawn(move || {
        let _ = runtime.start();
    });
}

fn stop_retrieval_runtime(runtime: RetrievalRuntimeManager) {
    thread::spawn(move || {
        let _ = runtime.stop();
    });
}

fn restart_retrieval_runtime(runtime: RetrievalRuntimeManager) {
    thread::spawn(move || {
        if runtime.stop().is_ok() {
            let _ = runtime.start();
        }
    });
}

#[tauri::command]
fn embedding_runtime_status(
    runtimes: tauri::State<'_, RetrievalRuntimeRegistry>,
) -> Result<RetrievalRuntimeStatus, String> {
    retrieval_status_result(
        &runtimes.embedding,
        "Embedding runtime status is unavailable",
    )
}

#[tauri::command]
fn embedding_runtime_start(
    runtimes: tauri::State<'_, RetrievalRuntimeRegistry>,
) -> Result<RetrievalRuntimeStatus, String> {
    start_retrieval_runtime(runtimes.embedding.clone());

    retrieval_status_result(
        &runtimes.embedding,
        "Embedding runtime status is unavailable",
    )
}

#[tauri::command]
fn embedding_runtime_stop(
    runtimes: tauri::State<'_, RetrievalRuntimeRegistry>,
) -> Result<RetrievalRuntimeStatus, String> {
    stop_retrieval_runtime(runtimes.embedding.clone());

    retrieval_status_result(
        &runtimes.embedding,
        "Embedding runtime status is unavailable",
    )
}

#[tauri::command]
fn embedding_runtime_restart(
    runtimes: tauri::State<'_, RetrievalRuntimeRegistry>,
) -> Result<RetrievalRuntimeStatus, String> {
    restart_retrieval_runtime(runtimes.embedding.clone());

    retrieval_status_result(
        &runtimes.embedding,
        "Embedding runtime status is unavailable",
    )
}

#[tauri::command]
fn reranker_runtime_status(
    runtimes: tauri::State<'_, RetrievalRuntimeRegistry>,
) -> Result<RetrievalRuntimeStatus, String> {
    retrieval_status_result(&runtimes.reranker, "Reranker runtime status is unavailable")
}

#[tauri::command]
fn reranker_runtime_start(
    runtimes: tauri::State<'_, RetrievalRuntimeRegistry>,
) -> Result<RetrievalRuntimeStatus, String> {
    start_retrieval_runtime(runtimes.reranker.clone());

    retrieval_status_result(&runtimes.reranker, "Reranker runtime status is unavailable")
}

#[tauri::command]
fn reranker_runtime_stop(
    runtimes: tauri::State<'_, RetrievalRuntimeRegistry>,
) -> Result<RetrievalRuntimeStatus, String> {
    stop_retrieval_runtime(runtimes.reranker.clone());

    retrieval_status_result(&runtimes.reranker, "Reranker runtime status is unavailable")
}

#[tauri::command]
fn reranker_runtime_restart(
    runtimes: tauri::State<'_, RetrievalRuntimeRegistry>,
) -> Result<RetrievalRuntimeStatus, String> {
    restart_retrieval_runtime(runtimes.reranker.clone());

    retrieval_status_result(&runtimes.reranker, "Reranker runtime status is unavailable")
}

#[tauri::command]
async fn chat_send(
    request: chat_bridge::ChatSendRequest,
    state: tauri::State<'_, AppState>,
    khoj_runtime: tauri::State<'_, KhojRuntimeManager>,
    local_model_runtime: tauri::State<'_, LocalModelRuntimeManager>,
) -> Result<chat_bridge::ChatSendResponse, String> {
    let khoj = runtime_status_result(khoj_runtime.inner())?;
    let model = local_model_status_result(local_model_runtime.inner())?;

    if khoj.state != KhojRuntimeState::Ready {
        return Err("Khoj runtime is not ready".to_owned());
    }

    if model.state != LocalModelRuntimeState::Ready {
        return Err("Local Qwen runtime is not ready".to_owned());
    }

    let prepared = rag::prepare_chat_request(state.inner(), request).await?;

    if prepared.insufficient {
        return Ok(chat_bridge::ChatSendResponse {
            response:
                "There is not enough evidence in the selected sources to answer this question."
                    .to_owned(),
            references: serde_json::to_value(prepared.retrieval).unwrap_or(serde_json::Value::Null),
            usage: serde_json::Value::Null,
            images: Vec::new(),
            files: Vec::new(),
            mermaidjs_diagram: Vec::new(),
        });
    }

    chat_bridge::send_chat(prepared.request).await
}

#[tauri::command]
async fn chat_sessions(
    khoj_runtime: tauri::State<'_, KhojRuntimeManager>,
) -> Result<Vec<chat_bridge::ChatSessionDto>, String> {
    let khoj = runtime_status_result(khoj_runtime.inner())?;

    if khoj.state != KhojRuntimeState::Ready {
        return Err("Khoj runtime is not ready".to_owned());
    }

    chat_bridge::list_sessions().await
}

#[tauri::command]
async fn chat_history(
    conversation_id: String,
    state: tauri::State<'_, AppState>,
    khoj_runtime: tauri::State<'_, KhojRuntimeManager>,
) -> Result<chat_bridge::ChatHistoryDto, String> {
    let khoj = runtime_status_result(khoj_runtime.inner())?;

    if khoj.state != KhojRuntimeState::Ready {
        return Err("Khoj runtime is not ready".to_owned());
    }

    let mut history = chat_bridge::get_history(&conversation_id).await?;

    history.response.workspace_id = state
        .core()
        .rag_conversation_workspace(&conversation_id)
        .map_err(|error| {
            format!("Unable to load conversation workspace:                  {error}")
        })?;

    history.response.rag_by_turn = state.core().rag_turns(&conversation_id).map_err(|error| {
        format!("Unable to load conversation evidence:                  {error}")
    })?;

    Ok(history)
}

#[tauri::command]
async fn chat_stream_start(
    app: tauri::AppHandle,
    request_id: String,
    request: chat_bridge::ChatSendRequest,
    state: tauri::State<'_, AppState>,
    streams: tauri::State<'_, chat_bridge::ChatStreamManager>,
    khoj_runtime: tauri::State<'_, KhojRuntimeManager>,
    local_model_runtime: tauri::State<'_, LocalModelRuntimeManager>,
) -> Result<chat_bridge::ChatStreamStartResult, String> {
    let khoj = runtime_status_result(khoj_runtime.inner())?;
    let model = local_model_status_result(local_model_runtime.inner())?;

    if khoj.state != KhojRuntimeState::Ready {
        return Err("R2H Intelligence Core is not ready".to_owned());
    }

    if model.state != LocalModelRuntimeState::Ready {
        return Err("Local Qwen runtime is not ready".to_owned());
    }

    if request.mode.is_grounded() {
        if let (Some(conversation_id), Some(workspace_id)) = (
            request
                .conversation_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
            request
                .workspace_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
        ) {
            let workspace_id = parse_id(workspace_id, "workspace ID").map_err(|error| {
                format!("Invalid conversation workspace:                              {error:?}")
            })?;

            let selected_source_ids = request
                .selected_source_ids
                .iter()
                .map(|value| parse_id(value, "source ID"))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| {
                    format!("Invalid conversation source scope:                          {error:?}")
                })?;

            state
                .core()
                .bind_rag_conversation(
                    conversation_id,
                    workspace_id,
                    rag::mode_storage_name(request.mode),
                    &selected_source_ids,
                )
                .map_err(|_| {
                    "This conversation belongs to a                      different workspace. Start a new                      conversation before changing                      workspace."
                        .to_owned()
                })?;
        }

        let _ = app.emit(
            chat_bridge::CHAT_STREAM_EVENT,
            chat_bridge::ChatStreamEvent {
                request_id: request_id.clone(),
                kind: "status".to_owned(),
                data: serde_json::Value::String("Searching workspace…".to_owned()),
            },
        );
    }

    let prepared = rag::prepare_chat_request(state.inner(), request).await?;

    if let Some(retrieval) = &prepared.retrieval {
        let _ = app.emit(
            chat_bridge::CHAT_STREAM_EVENT,
            chat_bridge::ChatStreamEvent {
                request_id: request_id.clone(),
                kind: "rag-metadata".to_owned(),
                data: serde_json::to_value(retrieval).unwrap_or(serde_json::Value::Null),
            },
        );
    }

    if prepared.insufficient {
        let _ = app.emit(
            chat_bridge::CHAT_STREAM_EVENT,
            chat_bridge::ChatStreamEvent {
                request_id: request_id.clone(),
                kind: "response-start".to_owned(),
                data: serde_json::Value::Null,
            },
        );

        let _ = app.emit(
            chat_bridge::CHAT_STREAM_EVENT,
            chat_bridge::ChatStreamEvent {
                request_id: request_id.clone(),
                kind: "token".to_owned(),
                data: serde_json::Value::String(
                    "There is not enough evidence in the selected sources to answer this question."
                        .to_owned(),
                ),
            },
        );

        let _ = app.emit(
            chat_bridge::CHAT_STREAM_EVENT,
            chat_bridge::ChatStreamEvent {
                request_id: request_id.clone(),
                kind: "done".to_owned(),
                data: serde_json::json!({
                    "conversationId": prepared.request.conversation_id,
                    "turnId": null,
                    "localOnly": true
                }),
            },
        );

        return Ok(chat_bridge::ChatStreamStartResult { request_id });
    }

    let request = prepared.request;
    let retrieval = prepared.retrieval;
    let request_mode = request.mode;
    let request_workspace_id = request.workspace_id.clone();
    let selected_source_ids = request.selected_source_ids.clone();

    let cancellation = streams.register(request_id.clone())?;
    let stream_manager = streams.inner().clone();
    let task_request_id = request_id.clone();
    let task_state = state.inner().clone();

    tauri::async_runtime::spawn(async move {
        let result =
            chat_bridge::stream_chat(app.clone(), task_request_id.clone(), request, cancellation)
                .await;

        match result {
            Ok(completion) => {
                if !completion.cancelled
                    && let (
                        Some(conversation_id),
                        Some(turn_id),
                        Some(workspace_id),
                        Some(retrieval),
                    ) = (
                        completion.conversation_id,
                        completion.turn_id,
                        request_workspace_id,
                        retrieval,
                    )
                {
                    let persistence = (|| {
                        let workspace_id = parse_id(&workspace_id, "workspace ID")
                            .map_err(|error| format!("Invalid persisted workspace: {error:?}"))?;

                        let source_ids = selected_source_ids
                            .iter()
                            .map(|value| parse_id(value, "source ID"))
                            .collect::<Result<Vec<_>, _>>()
                            .map_err(|error| {
                                format!("Invalid persisted source scope: {error:?}")
                            })?;

                        task_state
                            .core()
                            .bind_rag_conversation(
                                &conversation_id,
                                workspace_id,
                                rag::mode_storage_name(request_mode),
                                &source_ids,
                            )
                            .map_err(|error| format!("Unable to bind RAG conversation: {error}"))?;

                        let value = serde_json::to_value(&retrieval).map_err(|error| {
                            format!("Unable to serialize RAG metadata: {error}")
                        })?;

                        task_state
                            .core()
                            .save_rag_turn(&conversation_id, &turn_id, workspace_id, &value)
                            .map_err(|error| format!("Unable to persist RAG metadata: {error}"))?;

                        Ok::<(), String>(())
                    })();

                    if let Err(error) = persistence {
                        let _ = app.emit(
                            chat_bridge::CHAT_STREAM_EVENT,
                            chat_bridge::ChatStreamEvent {
                                request_id: task_request_id.clone(),
                                kind: "error".to_owned(),
                                data: serde_json::Value::String(error),
                            },
                        );
                    }
                }
            }
            Err(error) => {
                let _ = app.emit(
                    chat_bridge::CHAT_STREAM_EVENT,
                    chat_bridge::ChatStreamEvent {
                        request_id: task_request_id.clone(),
                        kind: "error".to_owned(),
                        data: serde_json::Value::String(error),
                    },
                );
            }
        }

        stream_manager.remove(&task_request_id);
    });

    Ok(chat_bridge::ChatStreamStartResult { request_id })
}

#[tauri::command]
fn chat_stream_cancel(
    request_id: String,
    streams: tauri::State<'_, chat_bridge::ChatStreamManager>,
) -> Result<bool, String> {
    streams.cancel(&request_id)
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let application = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            if !matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                return;
            }

            let tauri::WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };

            api.prevent_close();

            if SHUTDOWN_STARTED.swap(true, Ordering::SeqCst) {
                return;
            }

            let app_handle = window.app_handle().clone();

            let chat_streams = app_handle
                .try_state::<chat_bridge::ChatStreamManager>()
                .map(|streams| streams.inner().clone());

            let embedding_worker = app_handle
                .try_state::<EmbeddingWorkerManager>()
                .map(|worker| worker.inner().clone());

            let local_model_runtime = app_handle
                .try_state::<LocalModelRuntimeManager>()
                .map(|runtime| runtime.inner().clone());

            let retrieval_runtimes = app_handle
                .try_state::<RetrievalRuntimeRegistry>()
                .map(|runtimes| runtimes.inner().clone());

            let khoj_runtime = app_handle
                .try_state::<KhojRuntimeManager>()
                .map(|runtime| runtime.inner().clone());

            thread::spawn(move || {
                if let Some(streams) = chat_streams {
                    streams.cancel_all();
                }

                if let Some(worker) = embedding_worker {
                    let _ = worker.stop();
                }

                if let Some(runtimes) = retrieval_runtimes {
                    let _ = runtimes.reranker.stop();
                    let _ = runtimes.embedding.stop();
                }

                if let Some(runtime) = local_model_runtime {
                    let _ = runtime.stop();
                }

                if let Some(runtime) = khoj_runtime {
                    let _ = runtime.stop();
                }

                app_handle.exit(0);
            });
        })
        .setup(|app| {
            let base_dir = env::var_os("R2H_SECOND_BRAIN_DATA_ROOT")
                .map(PathBuf::from)
                .unwrap_or(app.path().app_data_dir()?);
            let import_root = approved_import_root(app)?;
            let app_state = AppState::open(
                AppConfig::new(base_dir)?.with_allowed_import_roots(vec![import_root]),
            )?;
            app.manage(app_state.clone());

            let project_root = resolve_project_root()?;
            let ai_pack = R2hAiPackResolver::from_environment();

            let khoj_runtime =
                KhojRuntimeManager::new(KhojRuntimeConfig::development(project_root.clone()));
            let startup_khoj_runtime = khoj_runtime.clone();
            app.manage(khoj_runtime);

            let local_model_runtime = LocalModelRuntimeManager::new(
                LocalModelRuntimeConfig::with_pack(project_root.clone(), ai_pack.clone()),
            );
            let startup_local_model_runtime = local_model_runtime.clone();
            let prime_broker =
                PrimeCapabilityBroker::for_local_generation(local_model_runtime.clone())
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
            app.manage(local_model_runtime);
            app.manage(prime_broker);

            let retrieval_runtimes =
                RetrievalRuntimeRegistry::development_with_pack(project_root, ai_pack);
            let startup_retrieval_runtimes = retrieval_runtimes.clone();
            app.manage(retrieval_runtimes);

            let embedding_worker = EmbeddingWorkerManager::default();
            let startup_embedding_worker = embedding_worker.clone();
            app.manage(embedding_worker);

            app.manage(chat_bridge::ChatStreamManager::default());

            thread::spawn(move || {
                let _ = startup_local_model_runtime.start();
            });

            thread::spawn(move || {
                let _ = startup_khoj_runtime.start();
            });

            let startup_embedding_runtime = startup_retrieval_runtimes.embedding.clone();
            thread::spawn(move || {
                if startup_embedding_runtime.start().is_ok() {
                    let _ = startup_embedding_worker.start(app_state);
                }
            });

            let startup_reranker_runtime = startup_retrieval_runtimes.reranker.clone();
            thread::spawn(move || {
                let _ = startup_reranker_runtime.start();
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::workspaces::tauri_handlers::workspace_create,
            commands::workspaces::tauri_handlers::workspace_list,
            commands::ingestion::tauri_handlers::source_ingest_files,
            commands::ingestion::tauri_handlers::source_list,
            commands::ingestion::tauri_handlers::source_get,
            commands::search::tauri_handlers::search_execute,
            commands::search::tauri_handlers::citation_resolve,
            commands::audit::tauri_handlers::audit_list,
            commands::audit::tauri_handlers::integrity_verify,
            commands::maintenance::tauri_handlers::create_consistent_backup,
            commands::maintenance::tauri_handlers::verify_workspace_integrity,
            commands::maintenance::tauri_handlers::scan_orphan_blobs,
            commands::maintenance::tauri_handlers::restore_workspace_backup,
            khoj_runtime_status,
            khoj_runtime_start,
            khoj_runtime_stop,
            khoj_runtime_restart,
            local_model_runtime_status,
            local_model_runtime_start,
            local_model_runtime_stop,
            local_model_runtime_restart,
            embedding_runtime_status,
            embedding_runtime_start,
            embedding_runtime_stop,
            embedding_runtime_restart,
            reranker_runtime_status,
            reranker_runtime_start,
            reranker_runtime_stop,
            reranker_runtime_restart,
            chat_send,
            chat_sessions,
            chat_history,
            chat_stream_start,
            chat_stream_cancel,
        ])
        .build(tauri::generate_context!())?;

    application.run(|app_handle, event| {
        if matches!(
            event,
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
        ) {
            if let Some(streams) = app_handle.try_state::<chat_bridge::ChatStreamManager>() {
                streams.cancel_all();
            }

            if let Some(worker) = app_handle.try_state::<EmbeddingWorkerManager>() {
                let _ = worker.stop();
            }

            if let Some(runtimes) = app_handle.try_state::<RetrievalRuntimeRegistry>() {
                let _ = runtimes.reranker.stop();
                let _ = runtimes.embedding.stop();
            }

            if let Some(runtime) = app_handle.try_state::<LocalModelRuntimeManager>() {
                let _ = runtime.stop();
            }

            if let Some(runtime) = app_handle.try_state::<KhojRuntimeManager>() {
                let _ = runtime.stop();
            }
        }
    });

    Ok(())
}

fn resolve_project_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(root) = env::var_os("R2H_SECOND_BRAIN_PROJECT_ROOT") {
        return Ok(PathBuf::from(root));
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .ancestors()
        .nth(3)
        .map(PathBuf::from)
        .ok_or_else(|| "unable to resolve R2H project root".into())
}

fn approved_import_root<R: tauri::Runtime>(app: &tauri::App<R>) -> Result<PathBuf, tauri::Error> {
    app.path().document_dir()
}
