use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio_util::sync::CancellationToken;

use crate::rag::ChatKnowledgeMode;

const KHOJ_BASE_URL: &str = "http://127.0.0.1:42110";
const R2H_CHAT_CLIENT: &str = "r2h-desktop";
const KHOJ_STREAM_DELIMITER: &str = "␃🔚␗";
pub const CHAT_STREAM_EVENT: &str = "r2h-chat-stream";

#[derive(Debug, Clone, Deserialize)]
pub struct ChatSendRequest {
    pub query: String,
    pub conversation_id: Option<String>,
    pub create_new: bool,
    #[serde(default)]
    pub mode: ChatKnowledgeMode,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub selected_source_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSendResponse {
    pub response: String,
    #[serde(default)]
    pub references: Value,
    #[serde(default)]
    pub usage: Value,
    #[serde(default)]
    pub images: Vec<Value>,
    #[serde(default)]
    pub files: Vec<Value>,
    #[serde(rename = "mermaidjsDiagram", default)]
    pub mermaidjs_diagram: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSessionDto {
    pub conversation_id: String,
    pub slug: String,
    pub agent_name: Option<String>,
    pub created: String,
    pub updated: String,
    pub agent_icon: Option<String>,
    pub agent_color: Option<String>,
    pub agent_is_hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatHistoryDto {
    pub status: String,
    pub response: ChatHistoryResponseDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatHistoryResponseDto {
    #[serde(default)]
    pub chat: Vec<ChatHistoryMessageDto>,
    pub conversation_id: String,
    pub slug: String,
    #[serde(default)]
    pub agent: Value,
    pub is_owner: bool,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub rag_by_turn: std::collections::BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatHistoryMessageDto {
    pub by: String,
    #[serde(rename = "turnId")]
    pub turn_id: String,
    pub created: String,
    pub message: String,
    #[serde(default)]
    pub context: Vec<Value>,
    #[serde(default, rename = "trainOfThought")]
    pub train_of_thought: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamEvent {
    pub request_id: String,
    pub kind: String,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamStartResult {
    pub request_id: String,
}

#[derive(Debug, Clone)]
pub struct ChatStreamCompletion {
    pub conversation_id: Option<String>,
    pub turn_id: Option<String>,
    pub cancelled: bool,
}

#[derive(Clone, Default)]
pub struct ChatStreamManager {
    requests: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl ChatStreamManager {
    pub fn register(&self, request_id: String) -> Result<CancellationToken, String> {
        let mut requests = self
            .requests
            .lock()
            .map_err(|_| "Chat stream registry is unavailable".to_owned())?;

        if requests.contains_key(&request_id) {
            return Err("Chat request ID is already active".to_owned());
        }

        let cancellation = CancellationToken::new();
        requests.insert(request_id, cancellation.clone());

        Ok(cancellation)
    }

    pub fn cancel(&self, request_id: &str) -> Result<bool, String> {
        let requests = self
            .requests
            .lock()
            .map_err(|_| "Chat stream registry is unavailable".to_owned())?;

        if let Some(cancellation) = requests.get(request_id) {
            cancellation.cancel();
            return Ok(true);
        }

        Ok(false)
    }

    pub fn remove(&self, request_id: &str) {
        if let Ok(mut requests) = self.requests.lock() {
            requests.remove(request_id);
        }
    }

    pub fn cancel_all(&self) {
        if let Ok(requests) = self.requests.lock() {
            for cancellation in requests.values() {
                cancellation.cancel();
            }
        }
    }
}

fn client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        .no_proxy()
        .build()
        .map_err(|error| format!("Unable to initialize local chat client: {error}"))
}

async fn ensure_khoj_ready(client: &Client) -> Result<(), String> {
    let response = client
        .get(format!("{KHOJ_BASE_URL}/"))
        .send()
        .await
        .map_err(|error| format!("R2H Intelligence Core is unavailable: {error}"))?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "R2H Intelligence Core readiness check returned HTTP {}",
            response.status()
        ))
    }
}

fn build_body(request: &ChatSendRequest, stream: bool) -> Value {
    let mut body = serde_json::json!({
        "q": request.query.trim(),
        "stream": stream,
        "create_new": request.create_new,
    });

    if let Some(conversation_id) = &request.conversation_id
        && !conversation_id.trim().is_empty()
    {
        body["conversation_id"] = Value::String(conversation_id.trim().to_owned());
    }

    body
}

fn validate_request(request: &ChatSendRequest) -> Result<(), String> {
    let query = request.query.trim();

    if query.is_empty() {
        return Err("Chat query cannot be empty".to_owned());
    }

    if query.len() > 32_000 {
        return Err("Chat query exceeds the local request limit".to_owned());
    }

    Ok(())
}

fn emit_event(app: &AppHandle, request_id: &str, kind: &str, data: Value) -> Result<(), String> {
    app.emit(
        CHAT_STREAM_EVENT,
        ChatStreamEvent {
            request_id: request_id.to_owned(),
            kind: kind.to_owned(),
            data,
        },
    )
    .map_err(|error| format!("Unable to emit chat stream event: {error}"))
}

fn process_frame(
    app: &AppHandle,
    request_id: &str,
    frame: &str,
    llm_started: &mut bool,
    conversation_id: &mut Option<String>,
    turn_id: &mut Option<String>,
) -> Result<bool, String> {
    if frame.is_empty() {
        return Ok(false);
    }

    if let Ok(value) = serde_json::from_str::<Value>(frame)
        && let Some(kind) = value.get("type").and_then(Value::as_str)
    {
        let data = value.get("data").cloned().unwrap_or(Value::Null);

        match kind {
            "metadata" => {
                *conversation_id = data
                    .get("conversationId")
                    .and_then(Value::as_str)
                    .map(str::to_owned);

                *turn_id = data
                    .get("turnId")
                    .and_then(Value::as_str)
                    .map(str::to_owned);

                emit_event(app, request_id, "metadata", data)?;
            }
            "references" => {
                emit_event(app, request_id, "references", data)?;
            }
            "status" => {
                emit_event(app, request_id, "status", data)?;
            }
            "start_llm_response" => {
                *llm_started = true;
                emit_event(app, request_id, "response-start", Value::Null)?;
            }
            "end_llm_response" => {
                *llm_started = false;
                emit_event(app, request_id, "response-end", Value::Null)?;
            }
            "usage" => {
                emit_event(app, request_id, "usage", data)?;
            }
            "end_response" => {
                return Ok(true);
            }
            _ => {
                emit_event(app, request_id, "protocol", value)?;
            }
        }

        return Ok(false);
    }

    if *llm_started {
        emit_event(app, request_id, "token", Value::String(frame.to_owned()))?;
    }

    Ok(false)
}

pub async fn stream_chat(
    app: AppHandle,
    request_id: String,
    request: ChatSendRequest,
    cancellation: CancellationToken,
) -> Result<ChatStreamCompletion, String> {
    validate_request(&request)?;

    let client = client()?;
    ensure_khoj_ready(&client).await?;

    let response = client
        .post(format!("{KHOJ_BASE_URL}/api/chat?client={R2H_CHAT_CLIENT}"))
        .json(&build_body(&request, true))
        .send()
        .await
        .map_err(|error| format!("Local chat request failed: {error}"))?;

    let status = response.status();

    if !status.is_success() {
        let payload = response.text().await.unwrap_or_default();

        return Err(format!(
            "R2H Intelligence Core returned HTTP {status}: {payload}"
        ));
    }

    let mut stream = response.bytes_stream();
    let delimiter = KHOJ_STREAM_DELIMITER.as_bytes();
    let mut pending = Vec::<u8>::new();
    let mut llm_started = false;
    let mut conversation_id = None;
    let mut turn_id = None;
    let mut ended = false;

    loop {
        tokio::select! {
            _ = cancellation.cancelled() => {
                emit_event(
                    &app,
                    &request_id,
                    "cancelled",
                    serde_json::json!({
                        "conversationId": conversation_id,
                        "turnId": turn_id,
                    }),
                )?;

                return Ok(ChatStreamCompletion {
                    conversation_id,
                    turn_id,
                    cancelled: true,
                });
            }

            next = stream.next() => {
                match next {
                    Some(Ok(chunk)) => {
                        pending.extend_from_slice(&chunk);

                        while let Some(position) = pending
                            .windows(delimiter.len())
                            .position(|window| window == delimiter)
                        {
                            let frame = pending[..position].to_vec();

                            pending.drain(
                                ..position + delimiter.len()
                            );

                            let frame = String::from_utf8(frame)
                                .map_err(|error| {
                                    format!(
                                        "Invalid UTF-8 in chat stream: {error}"
                                    )
                                })?;

                            ended = process_frame(
                                &app,
                                &request_id,
                                &frame,
                                &mut llm_started,
                                &mut conversation_id,
                                &mut turn_id,
                            )?;

                            if ended {
                                break;
                            }
                        }

                        if ended {
                            break;
                        }
                    }

                    Some(Err(error)) => {
                        return Err(format!(
                            "Local chat stream failed: {error}"
                        ));
                    }

                    None => break,
                }
            }
        }
    }

    if !pending.is_empty() {
        let frame = String::from_utf8(pending)
            .map_err(|error| format!("Invalid final UTF-8 chat frame: {error}"))?;

        let _ = process_frame(
            &app,
            &request_id,
            &frame,
            &mut llm_started,
            &mut conversation_id,
            &mut turn_id,
        )?;
    }

    emit_event(
        &app,
        &request_id,
        "done",
        serde_json::json!({
            "conversationId": conversation_id,
            "turnId": turn_id,
        }),
    )?;

    Ok(ChatStreamCompletion {
        conversation_id,
        turn_id,
        cancelled: false,
    })
}

pub async fn send_chat(request: ChatSendRequest) -> Result<ChatSendResponse, String> {
    validate_request(&request)?;

    let client = client()?;
    ensure_khoj_ready(&client).await?;

    let response = client
        .post(format!("{KHOJ_BASE_URL}/api/chat?client={R2H_CHAT_CLIENT}"))
        .json(&build_body(&request, false))
        .send()
        .await
        .map_err(|error| format!("Local chat request failed: {error}"))?;

    let status = response.status();
    let payload = response
        .text()
        .await
        .map_err(|error| format!("Unable to read local chat response: {error}"))?;

    if !status.is_success() {
        return Err(format!(
            "R2H Intelligence Core returned HTTP {status}: {payload}"
        ));
    }

    serde_json::from_str(&payload).map_err(|error| format!("Invalid local chat response: {error}"))
}

pub async fn list_sessions() -> Result<Vec<ChatSessionDto>, String> {
    let client = client()?;
    ensure_khoj_ready(&client).await?;

    let response = client
        .get(format!("{KHOJ_BASE_URL}/api/chat/sessions"))
        .query(&[("client", R2H_CHAT_CLIENT), ("recent", "true")])
        .send()
        .await
        .map_err(|error| format!("Unable to load local conversations: {error}"))?;

    let status = response.status();
    let payload = response
        .text()
        .await
        .map_err(|error| format!("Unable to read local conversations: {error}"))?;

    if !status.is_success() {
        return Err(format!(
            "Conversation list returned HTTP {status}: {payload}"
        ));
    }

    if let Ok(values) = serde_json::from_str::<Vec<ChatSessionDto>>(&payload) {
        return Ok(values);
    }

    let single = serde_json::from_str::<ChatSessionDto>(&payload)
        .map_err(|error| format!("Invalid local conversation response: {error}"))?;

    Ok(vec![single])
}

pub async fn get_history(conversation_id: &str) -> Result<ChatHistoryDto, String> {
    let conversation_id = conversation_id.trim();

    if conversation_id.is_empty() {
        return Err("Conversation ID is required".to_owned());
    }

    let client = client()?;
    ensure_khoj_ready(&client).await?;

    let response = client
        .get(format!("{KHOJ_BASE_URL}/api/chat/history"))
        .query(&[
            ("client", R2H_CHAT_CLIENT),
            ("conversation_id", conversation_id),
        ])
        .send()
        .await
        .map_err(|error| format!("Unable to load conversation history: {error}"))?;

    let status = response.status();
    let payload = response
        .text()
        .await
        .map_err(|error| format!("Unable to read conversation history: {error}"))?;

    if !status.is_success() {
        return Err(format!(
            "Conversation history returned HTTP {status}: {payload}"
        ));
    }

    serde_json::from_str(&payload)
        .map_err(|error| format!("Invalid conversation history response: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_delimiter_matches_khoj_protocol() {
        assert_eq!(KHOJ_STREAM_DELIMITER, "␃🔚␗");
    }

    #[test]
    fn local_chat_endpoint_is_loopback_only() {
        assert_eq!(KHOJ_BASE_URL, "http://127.0.0.1:42110");
        assert!(!KHOJ_BASE_URL.contains("0.0.0.0"));
    }

    #[test]
    fn stream_manager_cancels_registered_request() -> Result<(), String> {
        let manager = ChatStreamManager::default();
        let token = manager.register("request-1".to_owned())?;

        assert!(!token.is_cancelled());
        assert!(manager.cancel("request-1")?);
        assert!(token.is_cancelled());

        Ok(())
    }
}
