// The Prime protocol surface is intentionally broader than the current broker
// wiring; the full upstream agent loop remains out of scope.
#![allow(dead_code)]

use std::fmt::{self, Display, Formatter};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use super::capability::{
    ALLOWED_METHODS, METHOD_BROKER_HELLO, METHOD_BROKER_PING, METHOD_PROVIDER_GENERATE,
};

pub(crate) const PROTOCOL_VERSION: &str = "r2h-prime-broker-v1";
pub(crate) const MAX_FRAME_BYTES: usize = 128 * 1024;
pub(crate) const MAX_REQUEST_ID_CHARS: usize = 128;
pub(crate) const MAX_SESSION_ID_BYTES: usize = 64;
pub(crate) const MAX_CAPABILITY_BYTES: usize = 256;
pub(crate) const MAX_METHOD_BYTES: usize = 64;
pub(crate) const MAX_PROMPT_CHARS: usize = 24_000;
pub(crate) const MAX_PROMPT_BYTES: usize = 96 * 1024;
pub(crate) const MAX_TOKENS: u32 = 256;
pub(crate) const MAX_TEMPERATURE: f64 = 2.0;
pub(crate) const MAX_PROVIDER_TEXT_BYTES: usize = 48 * 1024;
pub(crate) const MAX_MODEL_BYTES: usize = 128;
pub(crate) const MAX_FINISH_REASON_BYTES: usize = 64;
pub(crate) const MAX_USAGE_BYTES: usize = 8 * 1024;
pub(crate) const MAX_PROVIDER_ERROR_CODE_BYTES: usize = 128;
pub(crate) const MAX_PROVIDER_ERROR_MESSAGE_BYTES: usize = 2 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BrokerRequest {
    #[serde(rename = "protocolVersion")]
    pub(crate) protocol_version: String,
    pub(crate) request_id: String,
    pub(crate) capability: String,
    #[serde(rename = "sessionId")]
    pub(crate) session_id: String,
    pub(crate) method: String,
    pub(crate) payload: Value,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GenerateRequest {
    #[serde(rename = "requestId")]
    pub(crate) request_id: String,
    pub(crate) prompt: String,
    #[serde(rename = "maxTokens")]
    pub(crate) max_tokens: u32,
    pub(crate) temperature: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum ProviderStatus {
    #[serde(rename = "SUCCESS")]
    Success,
    #[serde(rename = "LOCAL_AI_NOT_READY")]
    LocalAiNotReady,
    #[serde(rename = "CAPABILITY_UNAVAILABLE")]
    CapabilityUnavailable,
    #[serde(rename = "INVALID_REQUEST")]
    InvalidRequest,
    #[serde(rename = "TIMEOUT")]
    Timeout,
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "RUNTIME_ERROR")]
    RuntimeError,
    #[serde(rename = "INTERNAL_ERROR")]
    InternalError,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum ResponseStatus {
    #[serde(rename = "SUCCESS")]
    Success,
    #[serde(rename = "LOCAL_AI_NOT_READY")]
    LocalAiNotReady,
    #[serde(rename = "CAPABILITY_UNAVAILABLE")]
    CapabilityUnavailable,
    #[serde(rename = "INVALID_REQUEST")]
    InvalidRequest,
    #[serde(rename = "TIMEOUT")]
    Timeout,
    #[serde(rename = "CANCELLED")]
    Cancelled,
    #[serde(rename = "RUNTIME_ERROR")]
    RuntimeError,
    #[serde(rename = "INTERNAL_ERROR")]
    InternalError,
    #[serde(rename = "BROKER_AUTH_FAILED")]
    BrokerAuthFailed,
    #[serde(rename = "BROKER_CAPABILITY_EXPIRED")]
    BrokerCapabilityExpired,
    #[serde(rename = "BROKER_CAPABILITY_REVOKED")]
    BrokerCapabilityRevoked,
    #[serde(rename = "BROKER_PROTOCOL_ERROR")]
    BrokerProtocolError,
    #[serde(rename = "BROKER_METHOD_NOT_ALLOWED")]
    BrokerMethodNotAllowed,
    #[serde(rename = "BROKER_REQUEST_TOO_LARGE")]
    BrokerRequestTooLarge,
    #[serde(rename = "BROKER_TRANSPORT_ERROR")]
    BrokerTransportError,
    #[serde(rename = "BROKER_OVERLOADED")]
    BrokerOverloaded,
}

impl From<ProviderStatus> for ResponseStatus {
    fn from(status: ProviderStatus) -> Self {
        match status {
            ProviderStatus::Success => Self::Success,
            ProviderStatus::LocalAiNotReady => Self::LocalAiNotReady,
            ProviderStatus::CapabilityUnavailable => Self::CapabilityUnavailable,
            ProviderStatus::InvalidRequest => Self::InvalidRequest,
            ProviderStatus::Timeout => Self::Timeout,
            ProviderStatus::Cancelled => Self::Cancelled,
            ProviderStatus::RuntimeError => Self::RuntimeError,
            ProviderStatus::InternalError => Self::InternalError,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProviderResult {
    #[serde(rename = "requestId")]
    pub(crate) request_id: String,
    pub(crate) status: ProviderStatus,
    pub(crate) text: String,
    pub(crate) model: String,
    #[serde(rename = "finishReason")]
    pub(crate) finish_reason: String,
    pub(crate) usage: Option<Value>,
    pub(crate) error: Option<ProviderError>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProviderError {
    pub(crate) code: String,
    pub(crate) message: String,
}

#[derive(Serialize)]
pub(crate) struct BrokerResponse {
    #[serde(rename = "protocolVersion")]
    pub(crate) protocol_version: &'static str,
    pub(crate) request_id: String,
    pub(crate) status: ResponseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) payload: Option<Value>,
}

#[derive(Clone)]
pub(crate) enum ValidatedOperation {
    Generate(GenerateRequest),
    Hello,
    Ping,
}

pub(crate) struct ValidatedRequest {
    pub(crate) request_id: String,
    pub(crate) session_id: String,
    pub(crate) capability: String,
    pub(crate) operation: ValidatedOperation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProtocolError {
    RequestTooLarge,
    MalformedJson,
    UnsupportedVersion,
    InvalidRequest,
    MethodNotAllowed,
}

impl Display for ProtocolError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::RequestTooLarge => "broker request is too large",
            Self::MalformedJson => "broker request is malformed",
            Self::UnsupportedVersion => "broker protocol version is unsupported",
            Self::InvalidRequest => "broker request is invalid",
            Self::MethodNotAllowed => "broker method is not allowed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProtocolError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProviderResultError {
    Invalid,
    Serialization,
}

impl Display for ProviderResultError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Invalid => "local provider returned an invalid result",
            Self::Serialization => "local provider result could not be serialized",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProviderResultError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FrameError {
    TooLarge,
    InvalidLength,
    Truncated,
    Io,
}

impl Display for FrameError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::TooLarge => "broker frame is too large",
            Self::InvalidLength => "broker frame length is invalid",
            Self::Truncated => "broker frame is truncated",
            Self::Io => "broker transport I/O failed",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for FrameError {}

pub(crate) fn decode_request(frame: &[u8]) -> Result<BrokerRequest, ProtocolError> {
    if frame.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::RequestTooLarge);
    }

    serde_json::from_slice(frame).map_err(|_| ProtocolError::MalformedJson)
}

pub(crate) fn validate_request(request: BrokerRequest) -> Result<ValidatedRequest, ProtocolError> {
    if request.protocol_version != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion);
    }
    validate_bounded_identifier(&request.request_id, MAX_REQUEST_ID_CHARS)?;
    validate_bounded_identifier(&request.session_id, MAX_SESSION_ID_BYTES)?;
    validate_bounded_identifier(&request.capability, MAX_CAPABILITY_BYTES)?;
    validate_bounded_identifier(&request.method, MAX_METHOD_BYTES)?;

    let operation = match request.method.as_str() {
        METHOD_PROVIDER_GENERATE => {
            let payload = serde_json::from_value::<GenerateRequest>(request.payload)
                .map_err(|_| ProtocolError::InvalidRequest)?;
            if payload.request_id != request.request_id {
                return Err(ProtocolError::InvalidRequest);
            }
            validate_generate_request(&payload)?;
            ValidatedOperation::Generate(payload)
        }
        METHOD_BROKER_HELLO => {
            validate_empty_payload(request.payload)?;
            ValidatedOperation::Hello
        }
        METHOD_BROKER_PING => {
            validate_empty_payload(request.payload)?;
            ValidatedOperation::Ping
        }
        _ => return Err(ProtocolError::MethodNotAllowed),
    };

    Ok(ValidatedRequest {
        request_id: request.request_id,
        session_id: request.session_id,
        capability: request.capability,
        operation,
    })
}

pub(crate) fn validate_generate_request(request: &GenerateRequest) -> Result<(), ProtocolError> {
    validate_bounded_identifier(&request.request_id, MAX_REQUEST_ID_CHARS)?;
    if request.prompt.trim().is_empty()
        || request.prompt.chars().count() > MAX_PROMPT_CHARS
        || request.prompt.len() > MAX_PROMPT_BYTES
    {
        return Err(ProtocolError::InvalidRequest);
    }
    if request.max_tokens == 0 || request.max_tokens > MAX_TOKENS {
        return Err(ProtocolError::InvalidRequest);
    }
    if !request.temperature.is_finite()
        || request.temperature < 0.0
        || request.temperature > MAX_TEMPERATURE
    {
        return Err(ProtocolError::InvalidRequest);
    }
    Ok(())
}

pub(crate) fn normalize_provider_result(
    result: ProviderResult,
    expected_request_id: &str,
) -> Result<ProviderResult, ProviderResultError> {
    if result.request_id != expected_request_id
        || result.request_id.chars().count() > MAX_REQUEST_ID_CHARS
    {
        return Err(ProviderResultError::Invalid);
    }
    if result.text.len() > MAX_PROVIDER_TEXT_BYTES {
        return Err(ProviderResultError::Invalid);
    }
    if result.model.len() > MAX_MODEL_BYTES {
        return Err(ProviderResultError::Invalid);
    }
    if result.finish_reason.len() > MAX_FINISH_REASON_BYTES {
        return Err(ProviderResultError::Invalid);
    }
    if let Some(error) = &result.error
        && (error.code.len() > MAX_PROVIDER_ERROR_CODE_BYTES
            || error.message.len() > MAX_PROVIDER_ERROR_MESSAGE_BYTES)
    {
        return Err(ProviderResultError::Invalid);
    }
    if let Some(usage) = &result.usage {
        if !usage.is_object() {
            return Err(ProviderResultError::Invalid);
        }
        let serialized =
            serde_json::to_vec(usage).map_err(|_| ProviderResultError::Serialization)?;
        if serialized.len() > MAX_USAGE_BYTES {
            return Err(ProviderResultError::Invalid);
        }
    }

    if result.status == ProviderStatus::Success {
        if result.text.trim().is_empty() || result.error.is_some() {
            return Err(ProviderResultError::Invalid);
        }
    } else if !result.text.is_empty() || result.error.is_none() {
        return Err(ProviderResultError::Invalid);
    }

    Ok(result)
}

pub(crate) fn provider_response(
    request_id: String,
    result: ProviderResult,
) -> Result<BrokerResponse, ProviderResultError> {
    let result = normalize_provider_result(result, &request_id)?;
    let status = ResponseStatus::from(result.status);
    let payload = serde_json::to_value(result).map_err(|_| ProviderResultError::Serialization)?;

    Ok(BrokerResponse {
        protocol_version: PROTOCOL_VERSION,
        request_id,
        status,
        payload: Some(payload),
    })
}

pub(crate) fn status_response(request_id: String, status: ResponseStatus) -> BrokerResponse {
    BrokerResponse {
        protocol_version: PROTOCOL_VERSION,
        request_id,
        status,
        payload: None,
    }
}

pub(crate) fn lifecycle_response(
    request_id: String,
    operation: &ValidatedOperation,
) -> BrokerResponse {
    let payload = match operation {
        ValidatedOperation::Hello => serde_json::json!({
            "protocolVersion": PROTOCOL_VERSION,
            "allowedMethods": ALLOWED_METHODS,
        }),
        ValidatedOperation::Ping => serde_json::json!({
            "protocolVersion": PROTOCOL_VERSION,
        }),
        ValidatedOperation::Generate(_) => serde_json::Value::Null,
    };

    BrokerResponse {
        protocol_version: PROTOCOL_VERSION,
        request_id,
        status: ResponseStatus::Success,
        payload: Some(payload),
    }
}

pub(crate) fn encode_json(value: &BrokerResponse) -> Result<Vec<u8>, FrameError> {
    serde_json::to_vec(value).map_err(|_| FrameError::Io)
}

pub(crate) async fn read_frame<R>(reader: &mut R) -> Result<Option<Vec<u8>>, FrameError>
where
    R: AsyncRead + Unpin,
{
    read_frame_with_limit(reader, MAX_FRAME_BYTES).await
}

pub(crate) async fn read_frame_with_limit<R>(
    reader: &mut R,
    max_frame_bytes: usize,
) -> Result<Option<Vec<u8>>, FrameError>
where
    R: AsyncRead + Unpin,
{
    let mut first = [0_u8; 1];
    let read = reader.read(&mut first).await.map_err(|_| FrameError::Io)?;
    if read == 0 {
        return Ok(None);
    }

    let mut length_bytes = [0_u8; 4];
    length_bytes[0] = first[0];
    reader
        .read_exact(&mut length_bytes[1..])
        .await
        .map_err(|_| FrameError::Truncated)?;

    let length = u32::from_le_bytes(length_bytes) as usize;
    if length == 0 {
        return Err(FrameError::InvalidLength);
    }
    if length > max_frame_bytes {
        return Err(FrameError::TooLarge);
    }

    let mut frame = vec![0_u8; length];
    reader
        .read_exact(&mut frame)
        .await
        .map_err(|_| FrameError::Truncated)?;
    Ok(Some(frame))
}

pub(crate) async fn write_frame<W>(writer: &mut W, frame: &[u8]) -> Result<(), FrameError>
where
    W: AsyncWrite + Unpin,
{
    write_frame_with_limit(writer, frame, MAX_FRAME_BYTES).await
}

pub(crate) async fn write_frame_with_limit<W>(
    writer: &mut W,
    frame: &[u8],
    max_frame_bytes: usize,
) -> Result<(), FrameError>
where
    W: AsyncWrite + Unpin,
{
    if frame.is_empty() {
        return Err(FrameError::InvalidLength);
    }
    if frame.len() > max_frame_bytes {
        return Err(FrameError::TooLarge);
    }

    let length = u32::try_from(frame.len()).map_err(|_| FrameError::TooLarge)?;
    writer
        .write_all(&length.to_le_bytes())
        .await
        .map_err(|_| FrameError::Io)?;
    writer.write_all(frame).await.map_err(|_| FrameError::Io)?;
    writer.flush().await.map_err(|_| FrameError::Io)
}

fn validate_empty_payload(payload: Value) -> Result<(), ProtocolError> {
    let Value::Object(object) = payload else {
        return Err(ProtocolError::InvalidRequest);
    };
    if object.is_empty() {
        Ok(())
    } else {
        Err(ProtocolError::InvalidRequest)
    }
}

fn validate_bounded_identifier(value: &str, max_chars: usize) -> Result<(), ProtocolError> {
    if value.trim().is_empty()
        || value.chars().count() > max_chars
        || value.chars().any(char::is_control)
    {
        return Err(ProtocolError::InvalidRequest);
    }
    Ok(())
}

pub(crate) fn map_protocol_error(error: ProtocolError) -> ResponseStatus {
    match error {
        ProtocolError::RequestTooLarge => ResponseStatus::BrokerRequestTooLarge,
        ProtocolError::MalformedJson
        | ProtocolError::UnsupportedVersion
        | ProtocolError::InvalidRequest => ResponseStatus::BrokerProtocolError,
        ProtocolError::MethodNotAllowed => ResponseStatus::BrokerMethodNotAllowed,
    }
}

pub(crate) fn map_frame_error(error: FrameError) -> ResponseStatus {
    match error {
        FrameError::TooLarge => ResponseStatus::BrokerRequestTooLarge,
        FrameError::InvalidLength | FrameError::Truncated => ResponseStatus::BrokerProtocolError,
        FrameError::Io => ResponseStatus::BrokerTransportError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(payload: Value) -> BrokerRequest {
        BrokerRequest {
            protocol_version: PROTOCOL_VERSION.to_owned(),
            request_id: "request-1".to_owned(),
            capability: "capability".to_owned(),
            session_id: "session-1".to_owned(),
            method: METHOD_PROVIDER_GENERATE.to_owned(),
            payload,
        }
    }

    fn valid_payload() -> Value {
        serde_json::json!({
            "requestId": "request-1",
            "prompt": "bounded prompt",
            "maxTokens": 32,
            "temperature": 0.2,
        })
    }

    #[test]
    fn accepts_bounded_generate_request() -> Result<(), ProtocolError> {
        let validated = validate_request(request(valid_payload()))?;
        assert!(matches!(
            validated.operation,
            ValidatedOperation::Generate(_)
        ));
        Ok(())
    }

    #[test]
    fn rejects_prompt_above_local_character_limit() {
        let mut oversized = request(valid_payload());
        oversized.payload["prompt"] = Value::String("x".repeat(24_001));

        assert_eq!(
            validate_request(oversized).err(),
            Some(ProtocolError::InvalidRequest)
        );
    }

    #[test]
    fn rejects_max_tokens_above_local_limit() {
        let mut oversized = request(valid_payload());
        oversized.payload["maxTokens"] = serde_json::json!(257);

        assert_eq!(
            validate_request(oversized).err(),
            Some(ProtocolError::InvalidRequest)
        );
    }

    #[test]
    fn accepts_authoritative_success_envelope_with_null_error() {
        let result = serde_json::from_value::<ProviderResult>(serde_json::json!({
            "requestId": "request-1",
            "status": "SUCCESS",
            "text": "bounded result",
            "model": "qwen3-4b-q4km-generation",
            "finishReason": "stop",
            "usage": null,
            "error": null,
        }));

        assert!(result.is_ok());
    }

    #[test]
    fn accepts_authoritative_failure_envelope_with_bounded_error() {
        let result = serde_json::from_value::<ProviderResult>(serde_json::json!({
            "requestId": "request-1",
            "status": "LOCAL_AI_NOT_READY",
            "text": "",
            "model": "qwen3-4b-q4km-generation",
            "finishReason": "error",
            "usage": null,
            "error": {
                "code": "PRIME_PROVIDER_NOT_READY",
                "message": "Local AI is not ready",
            },
        }));

        assert!(result.is_ok());
    }

    #[test]
    fn rejects_oversized_provider_error_message() -> Result<(), Box<dyn std::error::Error>> {
        let result = serde_json::from_value::<ProviderResult>(serde_json::json!({
            "requestId": "request-1",
            "status": "INTERNAL_ERROR",
            "text": "",
            "model": "qwen3-4b-q4km-generation",
            "finishReason": "error",
            "usage": null,
            "error": {
                "code": "PRIME_PROVIDER_INTERNAL",
                "message": "x".repeat(2_049),
            },
        }))?;

        assert_eq!(
            normalize_provider_result(result, "request-1").err(),
            Some(ProviderResultError::Invalid)
        );
        Ok(())
    }

    #[test]
    fn rejects_oversized_provider_error_code_and_unknown_fields()
    -> Result<(), Box<dyn std::error::Error>> {
        let oversized_code = serde_json::from_value::<ProviderResult>(serde_json::json!({
            "requestId": "request-1",
            "status": "INTERNAL_ERROR",
            "text": "",
            "model": "qwen3-4b-q4km-generation",
            "finishReason": "error",
            "usage": null,
            "error": {
                "code": "x".repeat(129),
                "message": "provider failed",
            },
        }))?;
        assert_eq!(
            normalize_provider_result(oversized_code, "request-1").err(),
            Some(ProviderResultError::Invalid)
        );

        let unknown_field = serde_json::from_value::<ProviderResult>(serde_json::json!({
            "requestId": "request-1",
            "status": "SUCCESS",
            "text": "bounded result",
            "model": "qwen3-4b-q4km-generation",
            "finishReason": "stop",
            "usage": null,
            "error": null,
            "headers": {},
        }));
        assert!(unknown_field.is_err());
        Ok(())
    }

    #[test]
    fn rejects_wrong_request_id_oversized_text_and_invalid_error_shape()
    -> Result<(), Box<dyn std::error::Error>> {
        let result = serde_json::from_value::<ProviderResult>(serde_json::json!({
            "requestId": "other-request",
            "status": "SUCCESS",
            "text": "bounded result",
            "model": "qwen3-4b-q4km-generation",
            "finishReason": "stop",
            "usage": null,
            "error": null,
        }))?;
        assert_eq!(
            normalize_provider_result(result, "request-1").err(),
            Some(ProviderResultError::Invalid)
        );

        let oversized_text = serde_json::from_value::<ProviderResult>(serde_json::json!({
            "requestId": "request-1",
            "status": "SUCCESS",
            "text": "x".repeat(MAX_PROVIDER_TEXT_BYTES + 1),
            "model": "qwen3-4b-q4km-generation",
            "finishReason": "stop",
            "usage": null,
            "error": null,
        }))?;
        assert_eq!(
            normalize_provider_result(oversized_text, "request-1").err(),
            Some(ProviderResultError::Invalid)
        );

        let invalid_error = serde_json::from_value::<ProviderResult>(serde_json::json!({
            "requestId": "request-1",
            "status": "INTERNAL_ERROR",
            "text": "",
            "model": "qwen3-4b-q4km-generation",
            "finishReason": "error",
            "usage": null,
            "error": "invalid-error-shape",
        }));
        assert!(invalid_error.is_err());
        Ok(())
    }

    #[test]
    fn keeps_success_and_failure_error_semantics_structured()
    -> Result<(), Box<dyn std::error::Error>> {
        let success_with_error = ProviderResult {
            request_id: "request-1".to_owned(),
            status: ProviderStatus::Success,
            text: "bounded result".to_owned(),
            model: "qwen3-4b-q4km-generation".to_owned(),
            finish_reason: "stop".to_owned(),
            usage: None,
            error: Some(ProviderError {
                code: "UNEXPECTED_ERROR".to_owned(),
                message: "success cannot carry an error".to_owned(),
            }),
        };
        assert_eq!(
            normalize_provider_result(success_with_error, "request-1").err(),
            Some(ProviderResultError::Invalid)
        );

        let failure_without_error = ProviderResult {
            request_id: "request-1".to_owned(),
            status: ProviderStatus::Timeout,
            text: String::new(),
            model: "qwen3-4b-q4km-generation".to_owned(),
            finish_reason: "timeout".to_owned(),
            usage: None,
            error: None,
        };
        assert_eq!(
            normalize_provider_result(failure_without_error, "request-1").err(),
            Some(ProviderResultError::Invalid)
        );
        Ok(())
    }

    #[test]
    fn rejects_version_methods_and_unknown_fields() -> Result<(), Box<dyn std::error::Error>> {
        let mut unsupported = request(valid_payload());
        unsupported.protocol_version = "r2h-prime-broker-v0".to_owned();
        assert_eq!(
            validate_request(unsupported).err(),
            Some(ProtocolError::UnsupportedVersion)
        );

        let mut unknown_method = request(serde_json::json!({}));
        unknown_method.method = "/api/agent/run".to_owned();
        assert_eq!(
            validate_request(unknown_method).err(),
            Some(ProtocolError::MethodNotAllowed)
        );

        let encoded = serde_json::to_vec(&serde_json::json!({
            "protocolVersion": PROTOCOL_VERSION,
            "requestId": "request-1",
            "capability": "capability",
            "sessionId": "session-1",
            "method": METHOD_PROVIDER_GENERATE,
            "payload": {
                "requestId": "request-1",
                "prompt": "bounded prompt",
                "maxTokens": 32,
                "temperature": 0.2,
                "url": "http://127.0.0.1:42111/v1/chat/completions"
            },
            "url": "http://example.invalid"
        }))?;
        assert_eq!(
            decode_request(&encoded).err(),
            Some(ProtocolError::MalformedJson)
        );

        Ok(())
    }

    #[test]
    fn rejects_invalid_bounds() {
        let mut empty_prompt = request(valid_payload());
        empty_prompt.payload["prompt"] = Value::String("   ".to_owned());
        assert_eq!(
            validate_request(empty_prompt).err(),
            Some(ProtocolError::InvalidRequest)
        );

        let mut huge_prompt = request(valid_payload());
        huge_prompt.payload["prompt"] = Value::String("x".repeat(MAX_PROMPT_BYTES + 1));
        assert_eq!(
            validate_request(huge_prompt).err(),
            Some(ProtocolError::InvalidRequest)
        );

        let mut invalid_tokens = request(valid_payload());
        invalid_tokens.payload["maxTokens"] = serde_json::json!(0);
        assert_eq!(
            validate_request(invalid_tokens).err(),
            Some(ProtocolError::InvalidRequest)
        );

        let mut invalid_temperature = request(valid_payload());
        invalid_temperature.payload["temperature"] = serde_json::json!(2.1);
        assert_eq!(
            validate_request(invalid_temperature).err(),
            Some(ProtocolError::InvalidRequest)
        );
    }

    #[test]
    fn normalizes_all_provider_failure_statuses_without_success_text()
    -> Result<(), Box<dyn std::error::Error>> {
        let statuses = [
            ProviderStatus::LocalAiNotReady,
            ProviderStatus::CapabilityUnavailable,
            ProviderStatus::InvalidRequest,
            ProviderStatus::Timeout,
            ProviderStatus::Cancelled,
            ProviderStatus::RuntimeError,
            ProviderStatus::InternalError,
        ];

        for status in statuses {
            let result = ProviderResult {
                request_id: "request-1".to_owned(),
                status,
                text: String::new(),
                model: "qwen3-4b-q4km-generation".to_owned(),
                finish_reason: "error".to_owned(),
                usage: None,
                error: Some(ProviderError {
                    code: "PROVIDER_ERROR".to_owned(),
                    message: "provider failed".to_owned(),
                }),
            };
            let normalized = normalize_provider_result(result, "request-1")?;
            assert!(normalized.text.is_empty());
        }

        Ok(())
    }

    #[test]
    fn encodes_and_rejects_bounded_frames() -> Result<(), Box<dyn std::error::Error>> {
        let payload = serde_json::to_vec(&request(valid_payload()))?;
        assert!(payload.len() < MAX_FRAME_BYTES);
        assert!(matches!(write_frame_length_for_test(&payload), Ok(())));
        assert_eq!(
            write_frame_length_for_test(&vec![0_u8; MAX_FRAME_BYTES + 1]),
            Err(FrameError::TooLarge)
        );
        Ok(())
    }

    fn write_frame_length_for_test(frame: &[u8]) -> Result<(), FrameError> {
        if frame.is_empty() {
            return Err(FrameError::InvalidLength);
        }
        if frame.len() > MAX_FRAME_BYTES {
            return Err(FrameError::TooLarge);
        }
        Ok(())
    }
}
