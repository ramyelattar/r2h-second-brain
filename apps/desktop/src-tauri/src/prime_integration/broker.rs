// Broker keeps lifecycle surface beyond the currently registered provider.
#![allow(dead_code)]

use std::{
    fmt::{self, Display, Formatter},
    sync::Arc,
    time::Duration,
};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use super::{
    capability::{CapabilityError, CapabilityGrant, CapabilityLimits, CapabilityStore},
    local_ai_client::{LocalAiError, PrimeProviderClient, R2hLocalGenerationProvider},
    protocol::{
        BrokerResponse, FrameError, MAX_REQUEST_ID_CHARS, ResponseStatus, ValidatedOperation,
        decode_request, encode_json, lifecycle_response, map_frame_error, map_protocol_error,
        provider_response, read_frame, status_response, write_frame,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BrokerLimits {
    pub(crate) capability: CapabilityLimits,
    pub(crate) downstream_timeout: Duration,
}

impl Default for BrokerLimits {
    fn default() -> Self {
        Self {
            capability: CapabilityLimits::default(),
            downstream_timeout: Duration::from_secs(60),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrokerInitError {
    ProviderUnavailable,
}

impl Display for BrokerInitError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProviderUnavailable => {
                formatter.write_str("R2H local generation provider is unavailable")
            }
        }
    }
}

impl std::error::Error for BrokerInitError {}

#[derive(Clone)]
pub(crate) struct PrimeCapabilityBroker {
    sessions: CapabilityStore,
    provider: Arc<dyn PrimeProviderClient>,
    limits: BrokerLimits,
}

impl PrimeCapabilityBroker {
    pub(crate) fn for_local_generation(
        runtime: crate::local_model_runtime::LocalModelRuntimeManager,
    ) -> Result<Self, BrokerInitError> {
        let provider = R2hLocalGenerationProvider::new(runtime)
            .map_err(|_| BrokerInitError::ProviderUnavailable)?;
        Ok(Self::with_provider(
            Arc::new(provider),
            BrokerLimits::default(),
        ))
    }

    pub(crate) fn with_provider(
        provider: Arc<dyn PrimeProviderClient>,
        limits: BrokerLimits,
    ) -> Self {
        Self {
            sessions: CapabilityStore::new(limits.capability),
            provider,
            limits,
        }
    }

    pub(crate) fn create_session(&self) -> Result<CapabilityGrant, CapabilityError> {
        self.sessions.create_session()
    }

    pub(crate) fn revoke_session(&self, session_id: &str) -> Result<bool, CapabilityError> {
        self.sessions.revoke(session_id)
    }

    pub(crate) fn cleanup_sessions(&self) -> Result<usize, CapabilityError> {
        self.sessions.cleanup()
    }

    pub(crate) fn active_session_count(&self) -> Result<usize, CapabilityError> {
        self.sessions.active_session_count()
    }

    pub(crate) async fn handle_frame(
        &self,
        frame: &[u8],
        cancellation: CancellationToken,
    ) -> BrokerResponse {
        let request = match decode_request(frame) {
            Ok(request) => request,
            Err(error) => {
                return status_response(safe_request_id(None), map_protocol_error(error));
            }
        };
        let request_id = safe_request_id(Some(&request.request_id));
        let validated = match super::protocol::validate_request(request) {
            Ok(request) => request,
            Err(error) => return status_response(request_id, map_protocol_error(error)),
        };

        self.handle_validated(validated, cancellation).await
    }

    async fn handle_validated(
        &self,
        request: super::protocol::ValidatedRequest,
        cancellation: CancellationToken,
    ) -> BrokerResponse {
        let request_id = request.request_id.clone();
        let _lease = match self
            .sessions
            .acquire(&request.session_id, &request.capability)
        {
            Ok(lease) => lease,
            Err(error) => return status_response(request_id, map_capability_error(error)),
        };

        if cancellation.is_cancelled() {
            return status_response(request_id, ResponseStatus::Cancelled);
        }

        match request.operation {
            ValidatedOperation::Hello | ValidatedOperation::Ping => {
                lifecycle_response(request_id, &request.operation)
            }
            ValidatedOperation::Generate(generate) => {
                self.handle_generate(request_id, generate, cancellation)
                    .await
            }
        }
    }

    async fn handle_generate(
        &self,
        request_id: String,
        request: super::protocol::GenerateRequest,
        cancellation: CancellationToken,
    ) -> BrokerResponse {
        let downstream = self.provider.generate(&request, cancellation.clone());
        let result = timeout(self.limits.downstream_timeout, async {
            tokio::select! {
                _ = cancellation.cancelled() => Err(LocalAiError::Cancelled),
                result = downstream => result,
            }
        })
        .await;

        match result {
            Ok(Ok(provider_result)) => match provider_response(request_id, provider_result) {
                Ok(response) => response,
                Err(_) => status_response(request.request_id, ResponseStatus::RuntimeError),
            },
            Ok(Err(error)) => status_response(request_id, map_local_ai_error(error)),
            Err(_) => status_response(request_id, ResponseStatus::Timeout),
        }
    }

    pub(crate) async fn serve_connection<S>(
        &self,
        stream: &mut S,
        shutdown: CancellationToken,
    ) -> Result<(), FrameError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let (mut reader, mut writer) = tokio::io::split(&mut *stream);

        loop {
            let frame = tokio::select! {
                _ = shutdown.cancelled() => return Ok(()),
                frame = read_frame(&mut reader) => match frame {
                    Ok(frame) => frame,
                    Err(error) => {
                        let response = status_response("unknown".to_owned(), map_frame_error(error));
                        let payload = encode_json(&response)?;
                        write_frame(&mut writer, &payload).await?;
                        return Ok(());
                    }
                },
            };
            let Some(frame) = frame else {
                return Ok(());
            };

            let cancellation = CancellationToken::new();
            let request_future = self.handle_frame(&frame, cancellation.clone());
            tokio::pin!(request_future);
            let mut disconnect_probe = [0_u8; 1];

            let response = tokio::select! {
                _ = shutdown.cancelled() => {
                    cancellation.cancel();
                    return Ok(());
                }
                result = &mut request_future => result,
                disconnected = reader.read(&mut disconnect_probe) => {
                    cancellation.cancel();
                    let _ = (&mut request_future).await;
                    match disconnected {
                        Ok(_) | Err(_) => return Ok(()),
                    }
                }
            };

            let payload = match encode_json(&response) {
                Ok(payload) => payload,
                Err(_) => encode_internal_error_frame(&response.request_id)?,
            };
            write_frame(&mut writer, &payload).await?;
        }
    }
}

fn encode_internal_error_frame(request_id: &str) -> Result<Vec<u8>, FrameError> {
    let response = status_response(request_id.to_owned(), ResponseStatus::InternalError);
    encode_json(&response)
}

fn safe_request_id(request_id: Option<&str>) -> String {
    let Some(request_id) = request_id else {
        return "unknown".to_owned();
    };
    if request_id.trim().is_empty()
        || request_id.chars().count() > MAX_REQUEST_ID_CHARS
        || request_id.chars().any(char::is_control)
    {
        return "unknown".to_owned();
    }
    request_id.to_owned()
}

fn map_capability_error(error: CapabilityError) -> ResponseStatus {
    match error {
        CapabilityError::Expired => ResponseStatus::BrokerCapabilityExpired,
        CapabilityError::Revoked => ResponseStatus::BrokerCapabilityRevoked,
        CapabilityError::RequestLimitReached => ResponseStatus::BrokerOverloaded,
        CapabilityError::UnknownSession | CapabilityError::InvalidSecret => {
            ResponseStatus::BrokerAuthFailed
        }
        CapabilityError::SessionLimitReached | CapabilityError::Unavailable => {
            ResponseStatus::InternalError
        }
    }
}

fn map_local_ai_error(error: LocalAiError) -> ResponseStatus {
    match error {
        LocalAiError::Cancelled => ResponseStatus::Cancelled,
        LocalAiError::Timeout => ResponseStatus::Timeout,
        LocalAiError::LocalAiNotReady => ResponseStatus::LocalAiNotReady,
        LocalAiError::CapabilityUnavailable => ResponseStatus::CapabilityUnavailable,
        LocalAiError::InvalidRequest => ResponseStatus::InvalidRequest,
        LocalAiError::ResponseTooLarge
        | LocalAiError::InvalidResponse
        | LocalAiError::Transport
        | LocalAiError::Runtime => ResponseStatus::RuntimeError,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use futures_util::future::BoxFuture;

    use super::{
        super::{
            capability::{CapabilityLimits, METHOD_PROVIDER_GENERATE},
            local_ai_client::{LocalAiError, PrimeProviderClient},
            protocol::{
                BrokerRequest, GenerateRequest, PROTOCOL_VERSION, ProviderResult, ProviderStatus,
                ResponseStatus,
            },
        },
        BrokerLimits, PrimeCapabilityBroker,
    };
    use tokio_util::sync::CancellationToken;

    #[derive(Clone, Copy)]
    enum FakeAction {
        Success,
        Error(LocalAiError),
        Delay(Duration),
    }

    struct FakeProvider {
        action: FakeAction,
        calls: AtomicUsize,
    }

    impl FakeProvider {
        fn new(action: FakeAction) -> Self {
            Self {
                action,
                calls: AtomicUsize::new(0),
            }
        }
    }

    impl PrimeProviderClient for FakeProvider {
        fn generate<'a>(
            &'a self,
            request: &'a GenerateRequest,
            cancellation: CancellationToken,
        ) -> BoxFuture<'a, Result<ProviderResult, LocalAiError>> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let action = self.action;
            Box::pin(async move {
                match action {
                    FakeAction::Success => Ok(ProviderResult {
                        request_id: request.request_id.clone(),
                        status: ProviderStatus::Success,
                        text: "bounded result".to_owned(),
                        model: "qwen3-4b-q4km-generation".to_owned(),
                        finish_reason: "stop".to_owned(),
                        usage: None,
                        error: None,
                    }),
                    FakeAction::Error(error) => Err(error),
                    FakeAction::Delay(duration) => {
                        tokio::select! {
                            _ = cancellation.cancelled() => Err(LocalAiError::Cancelled),
                            _ = tokio::time::sleep(duration) => Ok(ProviderResult {
                                request_id: request.request_id.clone(),
                                status: ProviderStatus::Success,
                                text: "delayed result".to_owned(),
                                model: "qwen3-4b-q4km-generation".to_owned(),
                                finish_reason: "stop".to_owned(),
                                usage: None,
                                error: None,
                            }),
                        }
                    }
                }
            })
        }
    }

    fn broker(fake: Arc<FakeProvider>, limits: BrokerLimits) -> PrimeCapabilityBroker {
        PrimeCapabilityBroker::with_provider(fake, limits)
    }

    fn request(grant: &super::super::capability::CapabilityGrant) -> BrokerRequest {
        BrokerRequest {
            protocol_version: PROTOCOL_VERSION.to_owned(),
            request_id: "request-1".to_owned(),
            capability: grant.capability().to_owned(),
            session_id: grant.session_id().to_owned(),
            method: METHOD_PROVIDER_GENERATE.to_owned(),
            payload: serde_json::json!({
                "requestId": "request-1",
                "prompt": "bounded prompt",
                "maxTokens": 32,
                "temperature": 0.2,
            }),
        }
    }

    fn default_limits() -> BrokerLimits {
        BrokerLimits {
            capability: CapabilityLimits::default(),
            downstream_timeout: Duration::from_secs(1),
        }
    }

    #[tokio::test]
    async fn valid_request_has_deterministic_success_schema()
    -> Result<(), Box<dyn std::error::Error>> {
        let fake = Arc::new(FakeProvider::new(FakeAction::Success));
        let broker = broker(fake.clone(), default_limits());
        let grant = broker.create_session()?;
        let frame = serde_json::to_vec(&request(&grant))?;
        let response = broker.handle_frame(&frame, CancellationToken::new()).await;

        assert_eq!(response.status, ResponseStatus::Success);
        assert_eq!(response.request_id, "request-1");
        assert!(response.payload.is_some());
        assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[tokio::test]
    async fn invalid_request_is_rejected_without_model_call()
    -> Result<(), Box<dyn std::error::Error>> {
        let fake = Arc::new(FakeProvider::new(FakeAction::Success));
        let broker = broker(fake.clone(), default_limits());
        let grant = broker.create_session()?;
        let mut invalid = request(&grant);
        invalid.payload["prompt"] = serde_json::Value::String(String::new());

        let response = broker
            .handle_frame(&serde_json::to_vec(&invalid)?, CancellationToken::new())
            .await;
        assert_eq!(response.status, ResponseStatus::BrokerProtocolError);
        assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
        Ok(())
    }

    #[tokio::test]
    async fn timeout_and_cancellation_never_return_success()
    -> Result<(), Box<dyn std::error::Error>> {
        let fake = Arc::new(FakeProvider::new(FakeAction::Delay(Duration::from_secs(1))));
        let broker = broker(
            fake,
            BrokerLimits {
                downstream_timeout: Duration::from_millis(10),
                ..default_limits()
            },
        );
        let grant = broker.create_session()?;
        let frame = serde_json::to_vec(&request(&grant))?;
        let timeout_response = broker.handle_frame(&frame, CancellationToken::new()).await;
        assert_eq!(timeout_response.status, ResponseStatus::Timeout);

        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let cancelled_response = broker.handle_frame(&frame, cancellation).await;
        assert_eq!(cancelled_response.status, ResponseStatus::Cancelled);
        assert_ne!(cancelled_response.status, ResponseStatus::Success);
        Ok(())
    }

    #[tokio::test]
    async fn runtime_failure_maps_without_fabricated_result()
    -> Result<(), Box<dyn std::error::Error>> {
        let fake = Arc::new(FakeProvider::new(FakeAction::Error(LocalAiError::Runtime)));
        let broker = broker(fake, default_limits());
        let grant = broker.create_session()?;
        let response = broker
            .handle_frame(
                &serde_json::to_vec(&request(&grant))?,
                CancellationToken::new(),
            )
            .await;
        assert_eq!(response.status, ResponseStatus::RuntimeError);
        assert!(response.payload.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn per_session_concurrency_is_rejected_explicitly()
    -> Result<(), Box<dyn std::error::Error>> {
        let fake = Arc::new(FakeProvider::new(FakeAction::Delay(Duration::from_millis(
            50,
        ))));
        let broker = broker(fake, default_limits());
        let grant = broker.create_session()?;
        let frame = serde_json::to_vec(&request(&grant))?;
        let first_broker = broker.clone();
        let first_frame = frame.clone();
        let first = tokio::spawn(async move {
            first_broker
                .handle_frame(&first_frame, CancellationToken::new())
                .await
        });
        tokio::time::sleep(Duration::from_millis(5)).await;
        let second = broker.handle_frame(&frame, CancellationToken::new()).await;
        assert_eq!(second.status, ResponseStatus::BrokerOverloaded);
        let first = first.await?;
        assert_eq!(first.status, ResponseStatus::Success);
        Ok(())
    }

    #[tokio::test]
    async fn revoked_session_blocks_requests() -> Result<(), Box<dyn std::error::Error>> {
        let fake = Arc::new(FakeProvider::new(FakeAction::Success));
        let broker = broker(fake, default_limits());
        let grant = broker.create_session()?;
        assert!(broker.revoke_session(grant.session_id())?);
        let response = broker
            .handle_frame(
                &serde_json::to_vec(&request(&grant))?,
                CancellationToken::new(),
            )
            .await;
        assert_eq!(response.status, ResponseStatus::BrokerCapabilityRevoked);
        Ok(())
    }

    #[test]
    fn direct_generation_constructor_is_available_without_electron_or_tokens()
    -> Result<(), Box<dyn std::error::Error>> {
        let runtime = crate::local_model_runtime::LocalModelRuntimeManager::new(
            crate::local_model_runtime::LocalModelRuntimeConfig::development(
                std::path::PathBuf::from("project-root"),
            ),
        );
        let broker = PrimeCapabilityBroker::for_local_generation(runtime)?;
        assert_eq!(broker.active_session_count()?, 0);
        Ok(())
    }
}
