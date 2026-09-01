// Client surface covers the bounded request/response contract even where the
// current Prime path only consumes a subset.
#![allow(dead_code)]

use std::{
    fmt::{self, Display, Formatter},
    io,
    time::Duration,
};

use futures_util::{StreamExt, future::BoxFuture};
use reqwest::{Client, Request, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::local_model_runtime::{LocalModelRuntimeManager, LocalModelRuntimeState};

use super::protocol::{
    GenerateRequest, MAX_FRAME_BYTES, ProviderResult, ProviderStatus, normalize_provider_result,
    validate_generate_request,
};

pub(crate) const LOCAL_GENERATION_ENDPOINT: &str = "http://127.0.0.1:42111/v1/chat/completions";
const DEFAULT_PROVIDER_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LocalAiError {
    Cancelled,
    Timeout,
    LocalAiNotReady,
    CapabilityUnavailable,
    InvalidRequest,
    ResponseTooLarge,
    InvalidResponse,
    Transport,
    Runtime,
}

impl Display for LocalAiError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Cancelled => "local provider request was cancelled",
            Self::Timeout => "local provider request timed out",
            Self::LocalAiNotReady => "local provider is not ready",
            Self::CapabilityUnavailable => "local provider capability is unavailable",
            Self::InvalidRequest => "local provider rejected the request",
            Self::ResponseTooLarge => "local provider response is too large",
            Self::InvalidResponse => "local provider response is invalid",
            Self::Transport => "local provider transport failed",
            Self::Runtime => "local provider runtime failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for LocalAiError {}

pub(crate) trait PrimeProviderClient: Send + Sync {
    fn generate<'a>(
        &'a self,
        request: &'a GenerateRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'a, Result<ProviderResult, LocalAiError>>;
}

#[derive(Clone)]
pub(crate) struct R2hLocalGenerationProvider {
    client: Client,
    runtime: LocalModelRuntimeManager,
    endpoint: String,
    timeout: Duration,
}

impl R2hLocalGenerationProvider {
    pub(crate) fn new(runtime: LocalModelRuntimeManager) -> Result<Self, LocalAiError> {
        Self::with_endpoint(
            runtime,
            LOCAL_GENERATION_ENDPOINT.to_owned(),
            DEFAULT_PROVIDER_TIMEOUT,
        )
    }

    fn with_endpoint(
        runtime: LocalModelRuntimeManager,
        endpoint: String,
        timeout: Duration,
    ) -> Result<Self, LocalAiError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| LocalAiError::Runtime)?;

        Ok(Self {
            client,
            runtime,
            endpoint,
            timeout,
        })
    }

    fn build_request(
        &self,
        request: &GenerateRequest,
        model: &str,
    ) -> Result<Request, LocalAiError> {
        validate_generate_request(request).map_err(|_| LocalAiError::InvalidRequest)?;

        let payload = ChatCompletionsRequest {
            model: model.to_owned(),
            messages: vec![ChatMessage {
                role: "user",
                content: request.prompt.clone(),
            }],
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stream: false,
        };

        self.client
            .post(&self.endpoint)
            .json(&payload)
            .build()
            .map_err(|_| LocalAiError::Runtime)
    }

    fn parse_response(
        &self,
        request: &GenerateRequest,
        fallback_model: &str,
        body: &[u8],
    ) -> Result<ProviderResult, LocalAiError> {
        if body.len() > MAX_FRAME_BYTES {
            return Err(LocalAiError::ResponseTooLarge);
        }

        let response = serde_json::from_slice::<ChatCompletionsResponse>(body)
            .map_err(|_| LocalAiError::InvalidResponse)?;
        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or(LocalAiError::InvalidResponse)?;
        let text = choice
            .message
            .and_then(|message| message.content)
            .or(choice.text)
            .unwrap_or_default();

        if text.trim().is_empty() {
            return Err(LocalAiError::InvalidResponse);
        }

        let result = ProviderResult {
            request_id: request.request_id.clone(),
            status: ProviderStatus::Success,
            text,
            model: response
                .model
                .filter(|model| !model.trim().is_empty())
                .unwrap_or_else(|| fallback_model.to_owned()),
            finish_reason: choice
                .finish_reason
                .filter(|reason| !reason.trim().is_empty())
                .unwrap_or_else(|| "stop".to_owned()),
            usage: response.usage,
            error: None,
        };

        normalize_provider_result(result, &request.request_id)
            .map_err(|_| LocalAiError::InvalidResponse)
    }

    async fn execute(
        &self,
        request: &GenerateRequest,
        cancellation: CancellationToken,
    ) -> Result<ProviderResult, LocalAiError> {
        if cancellation.is_cancelled() {
            return Err(LocalAiError::Cancelled);
        }

        let runtime = self.runtime.clone();
        let startup = tokio::task::spawn_blocking(move || runtime.start());
        let startup_result = tokio::select! {
            _ = cancellation.cancelled() => {
                let runtime = self.runtime.clone();
                let _ = tokio::time::timeout(
                    Duration::from_secs(5),
                    tokio::task::spawn_blocking(move || runtime.stop()),
                ).await;
                return Err(LocalAiError::Cancelled);
            }
            result = startup => result.map_err(|_| LocalAiError::Runtime)?,
        };

        startup_result.map_err(map_runtime_start_error)?;

        let status = self.runtime.status().map_err(|_| LocalAiError::Runtime)?;
        if status.state != LocalModelRuntimeState::Ready {
            return Err(LocalAiError::LocalAiNotReady);
        }

        let outbound = self.build_request(request, &status.model_id)?;
        let response = tokio::time::timeout(self.timeout, async {
            tokio::select! {
                _ = cancellation.cancelled() => Err(LocalAiError::Cancelled),
                result = self.client.execute(outbound) => {
                    result.map_err(|_| LocalAiError::Transport)
                }
            }
        })
        .await
        .map_err(|_| LocalAiError::Timeout)??;

        let status_code = response.status();
        let body = read_bounded_body(response).await?;
        if !status_code.is_success() {
            return Err(map_http_status(status_code));
        }

        self.parse_response(request, &status.model_id, &body)
    }
}

impl PrimeProviderClient for R2hLocalGenerationProvider {
    fn generate<'a>(
        &'a self,
        request: &'a GenerateRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'a, Result<ProviderResult, LocalAiError>> {
        Box::pin(self.execute(request, cancellation))
    }
}

#[derive(Serialize)]
struct ChatCompletionsRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(rename = "max_tokens")]
    max_tokens: u32,
    temperature: f64,
    stream: bool,
}

#[derive(Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

#[derive(Deserialize)]
struct ChatCompletionsResponse {
    model: Option<String>,
    choices: Vec<ChatChoice>,
    usage: Option<Value>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: Option<ChatResponseMessage>,
    text: Option<String>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: Option<String>,
}

async fn read_bounded_body(response: reqwest::Response) -> Result<Vec<u8>, LocalAiError> {
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| LocalAiError::Transport)?;
        if body.len().saturating_add(chunk.len()) > MAX_FRAME_BYTES {
            return Err(LocalAiError::ResponseTooLarge);
        }
        body.extend_from_slice(&chunk);
    }

    Ok(body)
}

fn map_runtime_start_error(error: io::Error) -> LocalAiError {
    if error.kind() == io::ErrorKind::Interrupted {
        return LocalAiError::Cancelled;
    }

    if error.kind() == io::ErrorKind::NotFound
        || error.to_string().starts_with("AI_PACK_")
        || error.to_string().starts_with("MODEL_")
        || error.to_string().starts_with("GENERATION_MODEL_")
        || error.to_string().starts_with("LLAMA_RUNTIME_")
    {
        return LocalAiError::LocalAiNotReady;
    }

    LocalAiError::Runtime
}

fn map_http_status(status: StatusCode) -> LocalAiError {
    if status == StatusCode::REQUEST_TIMEOUT || status == StatusCode::GATEWAY_TIMEOUT {
        return LocalAiError::Timeout;
    }
    if status == StatusCode::SERVICE_UNAVAILABLE || status == StatusCode::BAD_GATEWAY {
        return LocalAiError::LocalAiNotReady;
    }
    if status == StatusCode::CONFLICT {
        return LocalAiError::CapabilityUnavailable;
    }
    if status.is_client_error() {
        return LocalAiError::InvalidRequest;
    }
    LocalAiError::Runtime
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::Duration};

    use super::*;
    use crate::local_model_runtime::{LocalModelRuntimeConfig, LocalModelRuntimeManager};

    fn request() -> GenerateRequest {
        GenerateRequest {
            request_id: "direct-request-1".to_owned(),
            prompt: "bounded prompt".to_owned(),
            max_tokens: 32,
            temperature: 0.2,
        }
    }

    fn provider() -> Result<R2hLocalGenerationProvider, LocalAiError> {
        let runtime = LocalModelRuntimeManager::new(LocalModelRuntimeConfig::development(
            PathBuf::from("project-root"),
        ));
        R2hLocalGenerationProvider::with_endpoint(
            runtime,
            "http://127.0.0.1:42111/v1/chat/completions".to_owned(),
            Duration::from_secs(1),
        )
    }

    #[test]
    fn direct_provider_targets_generation_endpoint_without_auth_headers() -> Result<(), LocalAiError>
    {
        let provider = provider()?;
        let outbound = provider.build_request(&request(), "generation-model")?;

        assert_eq!(
            outbound.url().as_str(),
            "http://127.0.0.1:42111/v1/chat/completions"
        );
        assert!(
            outbound
                .headers()
                .get(reqwest::header::AUTHORIZATION)
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn direct_provider_normalizes_openai_success_without_fabricating_text()
    -> Result<(), LocalAiError> {
        let provider = provider()?;
        let response = serde_json::json!({
            "id": "chatcmpl-direct-1",
            "model": "generation-model",
            "choices": [{
                "message": {"role": "assistant", "content": "bounded answer"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 4, "completion_tokens": 3, "total_tokens": 7}
        });

        let encoded = serde_json::to_vec(&response).map_err(|_| LocalAiError::InvalidResponse)?;
        let result = provider.parse_response(&request(), "generation-model", &encoded)?;

        assert_eq!(result.status, ProviderStatus::Success);
        assert_eq!(result.text, "bounded answer");
        assert_eq!(result.request_id, "direct-request-1");
        assert!(result.error.is_none());
        Ok(())
    }

    #[test]
    fn direct_provider_rejects_empty_openai_choices() -> Result<(), LocalAiError> {
        let provider = provider()?;
        let response = serde_json::json!({
            "model": "generation-model",
            "choices": [{"message": {"role": "assistant", "content": ""}}]
        });

        let encoded = serde_json::to_vec(&response).map_err(|_| LocalAiError::InvalidResponse)?;
        let result = provider.parse_response(&request(), "generation-model", &encoded);

        assert_eq!(result.err(), Some(LocalAiError::InvalidResponse));
        Ok(())
    }

    #[tokio::test]
    async fn direct_provider_observes_cancellation_before_runtime_start() -> Result<(), LocalAiError>
    {
        let provider = provider()?;
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let result = provider.generate(&request(), cancellation).await;

        assert_eq!(result.err(), Some(LocalAiError::Cancelled));
        Ok(())
    }

    #[test]
    fn failure_result_type_is_available_for_structured_errors() {
        let error = super::super::protocol::ProviderError {
            code: "PRIME_PROVIDER_NOT_READY".to_owned(),
            message: "ProgramData HTTP runtime is missing".to_owned(),
        };
        assert_eq!(error.code, "PRIME_PROVIDER_NOT_READY");
    }
}
