use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio_util::sync::CancellationToken;

use crate::{
    AppState,
    embedding_client::{EMBEDDING_MODEL_ID, EMBEDDING_MODEL_REVISION, EmbeddingClient},
};

const EMBEDDING_BATCH_LIMIT: u32 = 32;
const ACTIVE_RETRY_DELAY: Duration = Duration::from_secs(5);
const IDLE_SCAN_DELAY: Duration = Duration::from_secs(30);
const ERROR_RETRY_DELAY: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EmbeddingWorkerPass {
    pub discovered: usize,
    pub stored: usize,
    pub stale_deleted: u64,
}

#[derive(Default)]
struct EmbeddingWorkerState {
    running: bool,
    cancellation: Option<CancellationToken>,
}

#[derive(Clone, Default)]
pub struct EmbeddingWorkerManager {
    state: Arc<Mutex<EmbeddingWorkerState>>,
}

impl EmbeddingWorkerManager {
    pub fn start(&self, app_state: AppState) -> Result<bool, String> {
        let client = EmbeddingClient::local()?;
        let cancellation = CancellationToken::new();

        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "Embedding worker state is unavailable".to_owned())?;

            if state.running {
                return Ok(false);
            }

            state.running = true;
            state.cancellation = Some(cancellation.clone());
        }

        let manager = self.clone();

        tauri::async_runtime::spawn(async move {
            run_worker_loop(app_state, client, cancellation).await;

            manager.mark_finished();
        });

        Ok(true)
    }

    pub fn stop(&self) -> Result<bool, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "Embedding worker state is unavailable".to_owned())?;

        let Some(cancellation) = state.cancellation.as_ref() else {
            return Ok(false);
        };

        cancellation.cancel();
        Ok(true)
    }

    pub fn is_running(&self) -> Result<bool, String> {
        self.state
            .lock()
            .map(|state| state.running)
            .map_err(|_| "Embedding worker state is unavailable".to_owned())
    }

    fn mark_finished(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.running = false;
            state.cancellation = None;
        }
    }
}

async fn run_worker_loop(
    app_state: AppState,
    client: EmbeddingClient,
    cancellation: CancellationToken,
) {
    loop {
        if cancellation.is_cancelled() {
            break;
        }

        let delay = match run_worker_pass(&app_state, &client, &cancellation).await {
            Ok(pass) if pass.discovered == 0 => IDLE_SCAN_DELAY,
            Ok(_) => ACTIVE_RETRY_DELAY,
            Err(_) => ERROR_RETRY_DELAY,
        };

        tokio::select! {
            _ = cancellation.cancelled() => break,
            () = tokio::time::sleep(delay) => {}
        }
    }
}

async fn run_worker_pass(
    app_state: &AppState,
    client: &EmbeddingClient,
    cancellation: &CancellationToken,
) -> Result<EmbeddingWorkerPass, String> {
    let workspaces = app_state
        .core()
        .list_workspaces()
        .map_err(|error| format!("Unable to enumerate embedding workspaces: {error}"))?;

    let mut report = EmbeddingWorkerPass::default();

    for workspace in workspaces {
        if cancellation.is_cancelled() {
            break;
        }

        report.stale_deleted += app_state
            .core()
            .delete_stale_embeddings(workspace.id, EMBEDDING_MODEL_ID, EMBEDDING_MODEL_REVISION)
            .map_err(|error| {
                format!(
                    "Unable to delete stale embeddings for workspace {}: {error}",
                    workspace.id
                )
            })?;

        let pending = app_state
            .core()
            .pending_embeddings(
                workspace.id,
                EMBEDDING_MODEL_ID,
                EMBEDDING_MODEL_REVISION,
                EMBEDDING_BATCH_LIMIT,
            )
            .map_err(|error| {
                format!(
                    "Unable to load pending embeddings for workspace {}: {error}",
                    workspace.id
                )
            })?;

        report.discovered += pending.len();

        for item in pending {
            if cancellation.is_cancelled() {
                return Ok(report);
            }

            let generated = client.embed(&item.body).await?;
            let vector = convert_embedding(&generated)?;

            app_state
                .core()
                .store_embedding(
                    item.workspace_id,
                    item.block_id,
                    EMBEDDING_MODEL_ID,
                    EMBEDDING_MODEL_REVISION,
                    &vector,
                    &item.body_sha256,
                )
                .map_err(|error| {
                    format!(
                        "Unable to persist embedding for block {}: {error}",
                        item.block_id
                    )
                })?;

            report.stored += 1;
        }
    }

    Ok(report)
}

fn convert_embedding(values: &[f64]) -> Result<Vec<f32>, String> {
    let mut converted = Vec::with_capacity(values.len());

    for value in values {
        if !value.is_finite() {
            return Err("Embedding response contains a non-finite value".to_owned());
        }

        let converted_value = *value as f32;

        if !converted_value.is_finite() {
            return Err("Embedding value exceeds the persisted f32 range".to_owned());
        }

        converted.push(converted_value);
    }

    Ok(converted)
}

#[cfg(test)]
mod tests {
    use super::convert_embedding;

    #[test]
    fn converts_finite_embedding_values_to_f32() -> Result<(), String> {
        let converted = convert_embedding(&[0.25, -0.5, 1.0])?;

        assert_eq!(converted, vec![0.25_f32, -0.5_f32, 1.0_f32]);
        Ok(())
    }

    #[test]
    fn rejects_non_finite_embedding_values() {
        assert!(convert_embedding(&[f64::NAN]).is_err());
        assert!(convert_embedding(&[f64::INFINITY]).is_err());
    }

    #[test]
    fn rejects_values_outside_f32_range() {
        assert!(convert_embedding(&[f64::MAX]).is_err());
    }
}
