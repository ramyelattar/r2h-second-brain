from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(r"E:\Projects\r2h-second-brain")

APP = ROOT / "apps" / "desktop" / "src" / "App.tsx"
CLIENT = ROOT / "apps" / "desktop" / "src" / "api" / "client.ts"
CONTRACTS = ROOT / "apps" / "desktop" / "src" / "api" / "contracts.ts"
STYLES = ROOT / "apps" / "desktop" / "src" / "styles.css"

TAURI = ROOT / "apps" / "desktop" / "src-tauri"
LIB = TAURI / "src" / "lib.rs"
CHAT = TAURI / "src" / "chat_bridge.rs"
CARGO = TAURI / "Cargo.toml"
COMMAND_TESTS = TAURI / "tests" / "commands.rs"


def backup(path: Path) -> None:
    destination = path.with_name(path.name + ".before-phase-c4-3")

    if path.exists() and not destination.exists():
        destination.write_bytes(path.read_bytes())


def replace_once(
    text: str,
    old: str,
    new: str,
    label: str,
) -> str:
    count = text.count(old)

    if count != 1:
        raise RuntimeError(
            f"{label}: expected exactly one match, found {count}"
        )

    return text.replace(old, new, 1)


for path in (
    APP,
    CLIENT,
    CONTRACTS,
    STYLES,
    LIB,
    CHAT,
    CARGO,
    COMMAND_TESTS,
):
    backup(path)


# ============================================================
# Cargo
# ============================================================

cargo = CARGO.read_text(encoding="utf-8")

cargo = cargo.replace(
    'features = ["json", "rustls-tls"]',
    'features = ["json", "rustls-tls", "stream"]',
)

if 'futures-util = ' not in cargo:
    cargo = replace_once(
        cargo,
        'reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls", "stream"] }\n',
        'reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls", "stream"] }\n'
        'futures-util = "0.3"\n'
        'tokio-util = "0.7"\n',
        "streaming dependencies",
    )

CARGO.write_text(cargo, encoding="utf-8", newline="\n")


# ============================================================
# Rust bridge
# ============================================================

CHAT.write_text(
r'''use std::{
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

const KHOJ_BASE_URL: &str = "http://127.0.0.1:42110";
const R2H_CHAT_CLIENT: &str = "r2h-desktop";
const KHOJ_STREAM_DELIMITER: &str = "␃🔚␗";
pub const CHAT_STREAM_EVENT: &str = "r2h-chat-stream";

#[derive(Debug, Clone, Deserialize)]
pub struct ChatSendRequest {
    pub query: String,
    pub conversation_id: Option<String>,
    pub create_new: bool,
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

#[derive(Clone, Default)]
pub struct ChatStreamManager {
    requests: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl ChatStreamManager {
    pub fn register(
        &self,
        request_id: String,
    ) -> Result<CancellationToken, String> {
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
        .map_err(|error| {
            format!("Unable to initialize local chat client: {error}")
        })
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
        body["conversation_id"] =
            Value::String(conversation_id.trim().to_owned());
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

fn emit_event(
    app: &AppHandle,
    request_id: &str,
    kind: &str,
    data: Value,
) -> Result<(), String> {
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
        emit_event(
            app,
            request_id,
            "token",
            Value::String(frame.to_owned()),
        )?;
    }

    Ok(false)
}

pub async fn stream_chat(
    app: AppHandle,
    request_id: String,
    request: ChatSendRequest,
    cancellation: CancellationToken,
) -> Result<(), String> {
    validate_request(&request)?;

    let client = client()?;
    ensure_khoj_ready(&client).await?;

    let response = client
        .post(format!(
            "{KHOJ_BASE_URL}/api/chat?client={R2H_CHAT_CLIENT}"
        ))
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

                return Ok(());
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
            .map_err(|error| {
                format!("Invalid final UTF-8 chat frame: {error}")
            })?;

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

    Ok(())
}

pub async fn send_chat(
    request: ChatSendRequest,
) -> Result<ChatSendResponse, String> {
    validate_request(&request)?;

    let client = client()?;
    ensure_khoj_ready(&client).await?;

    let response = client
        .post(format!(
            "{KHOJ_BASE_URL}/api/chat?client={R2H_CHAT_CLIENT}"
        ))
        .json(&build_body(&request, false))
        .send()
        .await
        .map_err(|error| format!("Local chat request failed: {error}"))?;

    let status = response.status();
    let payload = response
        .text()
        .await
        .map_err(|error| {
            format!("Unable to read local chat response: {error}")
        })?;

    if !status.is_success() {
        return Err(format!(
            "R2H Intelligence Core returned HTTP {status}: {payload}"
        ));
    }

    serde_json::from_str(&payload)
        .map_err(|error| format!("Invalid local chat response: {error}"))
}

pub async fn list_sessions() -> Result<Vec<ChatSessionDto>, String> {
    let client = client()?;
    ensure_khoj_ready(&client).await?;

    let response = client
        .get(format!("{KHOJ_BASE_URL}/api/chat/sessions"))
        .query(&[
            ("client", R2H_CHAT_CLIENT),
            ("recent", "true"),
        ])
        .send()
        .await
        .map_err(|error| {
            format!("Unable to load local conversations: {error}")
        })?;

    let status = response.status();
    let payload = response.text().await.map_err(|error| {
        format!("Unable to read local conversations: {error}")
    })?;

    if !status.is_success() {
        return Err(format!(
            "Conversation list returned HTTP {status}: {payload}"
        ));
    }

    if let Ok(values) = serde_json::from_str::<Vec<ChatSessionDto>>(&payload) {
        return Ok(values);
    }

    let single = serde_json::from_str::<ChatSessionDto>(&payload)
        .map_err(|error| {
            format!("Invalid local conversation response: {error}")
        })?;

    Ok(vec![single])
}

pub async fn get_history(
    conversation_id: &str,
) -> Result<ChatHistoryDto, String> {
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
        .map_err(|error| {
            format!("Unable to load conversation history: {error}")
        })?;

    let status = response.status();
    let payload = response.text().await.map_err(|error| {
        format!("Unable to read conversation history: {error}")
    })?;

    if !status.is_success() {
        return Err(format!(
            "Conversation history returned HTTP {status}: {payload}"
        ));
    }

    serde_json::from_str(&payload)
        .map_err(|error| {
            format!("Invalid conversation history response: {error}")
        })
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
    fn stream_manager_cancels_registered_request() {
        let manager = ChatStreamManager::default();
        let token = manager.register("request-1".to_owned()).unwrap();

        assert!(!token.is_cancelled());
        assert!(manager.cancel("request-1").unwrap());
        assert!(token.is_cancelled());
    }
}
''',
    encoding="utf-8",
    newline="\n",
)


# ============================================================
# lib.rs
# ============================================================

lib = LIB.read_text(encoding="utf-8")

lib = replace_once(
    lib,
    "pub const COMMAND_NAMES: [&str; 24] = [",
    "pub const COMMAND_NAMES: [&str; 26] = [",
    "command registry count",
)

lib = replace_once(
    lib,
    '    "chat_history",\n];',
    '    "chat_history",\n'
    '    "chat_stream_start",\n'
    '    "chat_stream_cancel",\n'
    '];',
    "stream command names",
)

stream_commands = r'''
#[tauri::command]
async fn chat_stream_start(
    app: tauri::AppHandle,
    request_id: String,
    request: chat_bridge::ChatSendRequest,
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

    let cancellation = streams.register(request_id.clone())?;
    let stream_manager = streams.inner().clone();
    let task_request_id = request_id.clone();

    tauri::async_runtime::spawn(async move {
        let result = chat_bridge::stream_chat(
            app.clone(),
            task_request_id.clone(),
            request,
            cancellation,
        )
        .await;

        if let Err(error) = result {
            let _ = app.emit(
                chat_bridge::CHAT_STREAM_EVENT,
                chat_bridge::ChatStreamEvent {
                    request_id: task_request_id.clone(),
                    kind: "error".to_owned(),
                    data: serde_json::Value::String(error),
                },
            );
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

'''

lib = replace_once(
    lib,
    "pub fn run() -> Result<(), Box<dyn std::error::Error>> {",
    stream_commands
    + "pub fn run() -> Result<(), Box<dyn std::error::Error>> {",
    "stream commands",
)

lib = replace_once(
    lib,
    "            app.manage(local_model_runtime);\n",
    "            app.manage(local_model_runtime);\n"
    "            app.manage(chat_bridge::ChatStreamManager::default());\n",
    "stream manager state",
)

lib = replace_once(
    lib,
    "            chat_history,\n"
    "        ])",
    "            chat_history,\n"
    "            chat_stream_start,\n"
    "            chat_stream_cancel,\n"
    "        ])",
    "stream handler registration",
)

LIB.write_text(lib, encoding="utf-8", newline="\n")


# ============================================================
# command tests
# ============================================================

tests = COMMAND_TESTS.read_text(encoding="utf-8")

tests = replace_once(
    tests,
    "const APPROVED_COMMANDS: [&str; 24] = [",
    "const APPROVED_COMMANDS: [&str; 26] = [",
    "approved command count",
)

tests = replace_once(
    tests,
    '    "chat_history",\n];',
    '    "chat_history",\n'
    '    "chat_stream_start",\n'
    '    "chat_stream_cancel",\n'
    '];',
    "approved stream commands",
)

COMMAND_TESTS.write_text(tests, encoding="utf-8", newline="\n")


# ============================================================
# TypeScript contracts
# ============================================================

contracts = CONTRACTS.read_text(encoding="utf-8").rstrip()

contracts += r'''

export type ChatStreamEventKind =
  | "metadata"
  | "references"
  | "status"
  | "response-start"
  | "token"
  | "response-end"
  | "usage"
  | "done"
  | "cancelled"
  | "error"
  | "protocol";

export interface ChatStreamEventDto {
  requestId: string;
  kind: ChatStreamEventKind;
  data: unknown;
}

export interface ChatStreamStartResultDto {
  requestId: string;
}
'''

CONTRACTS.write_text(
    contracts + "\n",
    encoding="utf-8",
    newline="\n",
)


# ============================================================
# TypeScript client
# ============================================================

client = CLIENT.read_text(encoding="utf-8")

client = replace_once(
    client,
    "  ChatSessionDto,\n",
    "  ChatSessionDto,\n"
    "  ChatStreamStartResultDto,\n",
    "stream client type import",
)

client = client.rstrip() + r'''

export async function startChatStream(
  requestId: string,
  request: ChatSendRequestDto,
): Promise<ChatStreamStartResultDto> {
  return invoke<ChatStreamStartResultDto>("chat_stream_start", {
    requestId,
    request,
  });
}

export async function cancelChatStream(
  requestId: string,
): Promise<boolean> {
  return invoke<boolean>("chat_stream_cancel", { requestId });
}
'''

CLIENT.write_text(client + "\n", encoding="utf-8", newline="\n")


# ============================================================
# React imports
# ============================================================

app = APP.read_text(encoding="utf-8")

app = replace_once(
    app,
    'import { useEffect, useMemo, useState } from "react";',
    'import { useEffect, useMemo, useRef, useState } from "react";\n'
    'import { listen } from "@tauri-apps/api/event";',
    "React streaming imports",
)

app = replace_once(
    app,
    "  ChatSessionDto,\n",
    "  ChatSessionDto,\n"
    "  ChatStreamEventDto,\n",
    "stream event import",
)

new_chat_screen = r'''function ChatScreen({
  workspace,
  sources,
  khojRuntime,
  localModelRuntime,
}: {
  workspace: WorkspaceDto | null;
  sources: SourceDto[];
  khojRuntime: KhojRuntimeStatusDto;
  localModelRuntime: LocalModelRuntimeStatusDto;
}) {
  const [draft, setDraft] = useState("");
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [sessions, setSessions] = useState<ChatSessionDto[]>([]);
  const [messages, setMessages] = useState<ChatHistoryMessageDto[]>([]);
  const [activeRequestId, setActiveRequestId] = useState<string | null>(null);
  const [streamStatus, setStreamStatus] = useState("");
  const [loadingHistory, setLoadingHistory] = useState(false);
  const [error, setError] = useState("");

  const threadRef = useRef<HTMLDivElement | null>(null);
  const activeRequestRef = useRef<string | null>(null);
  const assistantTurnRef = useRef<string | null>(null);

  const runtimeReady =
    khojRuntime.state === "Ready" &&
    localModelRuntime.state === "Ready";

  const sending = activeRequestId !== null;

  async function refreshSessions(): Promise<ChatSessionDto[]> {
    if (khojRuntime.state !== "Ready") return [];

    const values = await client.listChatSessions();
    setSessions(values);
    return values;
  }

  async function reconcileConversation(id?: string | null) {
    let target = id ?? conversationId;
    const values = await refreshSessions();

    if (!target) {
      target = values[0]?.conversation_id ?? null;
    }

    if (!target) return;

    const history = await client.getChatHistory(target);
    setConversationId(history.response.conversation_id);
    setMessages(history.response.chat);
  }

  useEffect(() => {
    activeRequestRef.current = activeRequestId;
  }, [activeRequestId]);

  useEffect(() => {
    const element = threadRef.current;

    if (element) {
      element.scrollTo({
        top: element.scrollHeight,
        behavior: "smooth",
      });
    }
  }, [messages, streamStatus]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listen<ChatStreamEventDto>(
      "r2h-chat-stream",
      (event) => {
        if (disposed) return;

        const payload = event.payload;

        if (payload.requestId !== activeRequestRef.current) return;

        switch (payload.kind) {
          case "metadata": {
            const data = payload.data as {
              conversationId?: string;
              turnId?: string;
            };

            if (data.conversationId) {
              setConversationId(data.conversationId);
            }

            if (data.turnId) {
              assistantTurnRef.current = data.turnId;
            }

            break;
          }

          case "status":
            setStreamStatus(
              typeof payload.data === "string"
                ? payload.data.replaceAll("*", "")
                : "Generating locally…",
            );
            break;

          case "response-start": {
            const turnId =
              assistantTurnRef.current ?? crypto.randomUUID();

            assistantTurnRef.current = turnId;
            setStreamStatus("Writing response…");

            setMessages((current) => [
              ...current,
              {
                by: "r2h",
                turnId,
                created: new Date().toISOString(),
                message: "",
                context: [],
                trainOfThought: [],
              },
            ]);

            break;
          }

          case "token": {
            const token =
              typeof payload.data === "string" ? payload.data : "";

            const turnId = assistantTurnRef.current;

            if (!turnId || !token) break;

            setMessages((current) =>
              current.map((message) =>
                message.by !== "you" &&
                message.turnId === turnId
                  ? {
                      ...message,
                      message: message.message + token,
                    }
                  : message,
              ),
            );

            break;
          }

          case "done": {
            const data = payload.data as {
              conversationId?: string;
            };

            setActiveRequestId(null);
            setStreamStatus("");
            assistantTurnRef.current = null;

            void reconcileConversation(data.conversationId).catch(
              (reason: unknown) => {
                setError(errorMessage(reason));
              },
            );

            break;
          }

          case "cancelled": {
            const data = payload.data as {
              conversationId?: string;
            };

            setActiveRequestId(null);
            setStreamStatus("");
            assistantTurnRef.current = null;

            void reconcileConversation(data.conversationId).catch(
              (reason: unknown) => {
                setError(errorMessage(reason));
              },
            );

            break;
          }

          case "error":
            setActiveRequestId(null);
            setStreamStatus("");
            assistantTurnRef.current = null;
            setError(
              typeof payload.data === "string"
                ? payload.data
                : "Local generation failed.",
            );
            break;
        }
      },
    ).then((dispose) => {
      if (disposed) {
        dispose();
      } else {
        unlisten = dispose;
      }
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [conversationId, khojRuntime.state]);

  useEffect(() => {
    if (khojRuntime.state !== "Ready") {
      setSessions([]);
      return;
    }

    let active = true;

    void client
      .listChatSessions()
      .then((values) => {
        if (active) setSessions(values);
      })
      .catch((reason: unknown) => {
        if (active) setError(errorMessage(reason));
      });

    return () => {
      active = false;
    };
  }, [khojRuntime.state]);

  async function openSession(id: string) {
    if (sending) return;

    setLoadingHistory(true);
    setError("");

    try {
      const history = await client.getChatHistory(id);
      setConversationId(history.response.conversation_id);
      setMessages(history.response.chat);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setLoadingHistory(false);
    }
  }

  function newConversation() {
    if (sending) return;

    setConversationId(null);
    setMessages([]);
    setDraft("");
    setError("");
    setStreamStatus("");
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();

    const value = draft.trim();

    if (!value || !runtimeReady || sending) return;

    const requestId = crypto.randomUUID();
    const userTurnId = crypto.randomUUID();

    setDraft("");
    setError("");
    setStreamStatus("Preparing local generation…");
    setActiveRequestId(requestId);

    setMessages((current) => [
      ...current,
      {
        by: "you",
        turnId: userTurnId,
        created: new Date().toISOString(),
        message: value,
        context: [],
        trainOfThought: [],
      },
    ]);

    try {
      await client.startChatStream(requestId, {
        query: `/general ${value}`,
        conversation_id: conversationId,
        create_new: !conversationId,
      });
    } catch (reason) {
      setActiveRequestId(null);
      setStreamStatus("");
      setError(errorMessage(reason));
    }
  }

  async function stopGeneration() {
    const requestId = activeRequestRef.current;

    if (!requestId) return;

    setStreamStatus("Stopping generation…");

    try {
      const cancelled = await client.cancelChatStream(requestId);

      if (!cancelled) {
        setError("The active local request was not found.");
        setActiveRequestId(null);
        setStreamStatus("");
      }
    } catch (reason) {
      setError(errorMessage(reason));
    }
  }

  return (
    <div className="chat-layout">
      <section className="chat-workspace">
        <PageHeader
          title="AI Chat"
          description="Private local conversations powered by the embedded R2H intelligence runtime."
          action={
            <button
              type="button"
              className="quiet-button"
              onClick={newConversation}
              disabled={sending}
            >
              <Icon name="plus" /> New conversation
            </button>
          }
        />

        {error && <ErrorPanel message={error} />}

        <section className="glass-panel chat-thread">
          <div className="thread-heading">
            <div>
              <h2>
                {conversationId
                  ? sessions.find(
                      (session) =>
                        session.conversation_id === conversationId,
                    )?.slug ?? "Private conversation"
                  : "New private conversation"}
              </h2>
              <p>{workspace?.name ?? "Local knowledge environment"}</p>
            </div>

            <StatusPill tone={runtimeReady ? "success" : "warning"}>
              {runtimeReady ? "R2H AI ready" : "Runtime unavailable"}
            </StatusPill>
          </div>

          <div
            className="thread-body live-chat-thread"
            ref={threadRef}
          >
            {loadingHistory ? (
              <p className="loading-copy">Loading conversation…</p>
            ) : messages.length === 0 ? (
              <article className="assistant-message">
                <div className="message-role">
                  <Icon name="spark" /> R2H Second Brain
                </div>
                <h2>Ask your private second brain.</h2>
                <p>
                  Conversations run through the local R2H Intelligence
                  Core and remain stored on this device.
                </p>

                <div className="answer-grid">
                  <SignalRow
                    label="R2H Intelligence Core"
                    value={khojRuntimeLabel(khojRuntime)}
                  />
                  <SignalRow
                    label="Qwen3"
                    value={localModelRuntimeLabel(localModelRuntime)}
                  />
                  <SignalRow label="Sources" value={sources.length} />
                </div>
              </article>
            ) : (
              messages.map((message, index) =>
                message.by === "you" ? (
                  <article
                    className="user-prompt chat-message"
                    key={`${message.turnId}-user-${index}`}
                  >
                    <span>You</span>
                    <p>{message.message}</p>
                  </article>
                ) : (
                  <article
                    className="assistant-message chat-message"
                    key={`${message.turnId}-assistant-${index}`}
                  >
                    <div className="message-role">
                      <Icon name="spark" /> R2H Second Brain
                    </div>
                    <p>
                      {message.message || (
                        <span className="stream-caret">
                          Generating…
                        </span>
                      )}
                    </p>
                  </article>
                ),
              )
            )}

            {sending && streamStatus && (
              <div className="stream-status" role="status">
                <span className="local-dot" />
                {streamStatus}
              </div>
            )}
          </div>
        </section>

        <form className="chat-composer" onSubmit={submit}>
          <button
            type="button"
            aria-label="Attach context"
            disabled
          >
            <Icon name="plus" />
          </button>

          <input
            aria-label="Ask your second brain"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            placeholder={
              runtimeReady
                ? "Ask your second brain"
                : "Waiting for local AI runtime…"
            }
            disabled={!runtimeReady || sending}
          />

          <select aria-label="Local model" disabled>
            <option>Qwen3-4B · Local</option>
          </select>

          {sending ? (
            <button
              className="stop-generation-button"
              type="button"
              onClick={() => void stopGeneration()}
              aria-label="Stop generation"
            >
              <span aria-hidden="true">■</span>
              <span>Stop</span>
            </button>
          ) : (
            <button
              className="send-button"
              type="submit"
              disabled={!draft.trim() || !runtimeReady}
            >
              <Icon name="send" />
              <span className="sr-only">Send prompt</span>
            </button>
          )}
        </form>
      </section>

      <aside className="glass-panel context-drawer">
        <PanelHeading
          title="Conversations"
          meta={`${sessions.length} recent local sessions`}
        />

        <div className="chat-session-list">
          {sessions.length === 0 ? (
            <div className="context-state">
              <span>
                <Icon name="chat" />
              </span>
              <h3>No saved conversations</h3>
              <p>Your first local conversation will appear here.</p>
            </div>
          ) : (
            sessions.map((session) => (
              <button
                type="button"
                key={session.conversation_id}
                className={
                  session.conversation_id === conversationId
                    ? "chat-session selected"
                    : "chat-session"
                }
                onClick={() =>
                  void openSession(session.conversation_id)
                }
                disabled={sending || loadingHistory}
              >
                <span>
                  <strong>{session.slug}</strong>
                  <small>{session.updated}</small>
                </span>
                <Icon name="chevron" />
              </button>
            ))
          )}
        </div>

        <div className="drawer-section">
          <h3>Local intelligence</h3>
          <SignalRow
            label="R2H Intelligence Core"
            value={khojRuntimeLabel(khojRuntime)}
          />
          <SignalRow
            label="Qwen3"
            value={localModelRuntimeLabel(localModelRuntime)}
          />
          <SignalRow label="Network" value="Loopback only" />
          <SignalRow
            label="Conversation"
            value={conversationId ? "Saved locally" : "New"}
          />
        </div>
      </aside>
    </div>
  );
}
'''

pattern = re.compile(
    r"function ChatScreen\(\{.*?\n\}\n\nfunction ModelsScreen\(",
    re.DOTALL,
)

match = pattern.search(app)

if not match:
    raise RuntimeError("Unable to locate current ChatScreen")

app = (
    app[:match.start()]
    + new_chat_screen
    + "\n\nfunction ModelsScreen("
    + app[match.end():]
)

app = app.replace(
    "Khoj · ${khojRuntimeLabel(khojRuntime)}",
    "R2H Core · ${khojRuntimeLabel(khojRuntime)}",
)

app = app.replace(
    'label="Khoj service"',
    'label="R2H Intelligence Core"',
)

APP.write_text(app, encoding="utf-8", newline="\n")


# ============================================================
# CSS
# ============================================================

styles = STYLES.read_text(encoding="utf-8").rstrip()

styles += r'''

.stream-status {
  position: sticky;
  bottom: 0;
  display: flex;
  align-items: center;
  gap: 9px;
  width: fit-content;
  padding: 8px 12px;
  border: 1px solid rgb(152 93 246 / 35%);
  border-radius: 999px;
  background: rgb(11 11 18 / 92%);
  color: rgb(232 220 255 / 82%);
  font-size: 0.7rem;
  backdrop-filter: blur(14px);
}

.stream-caret {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--muted);
}

.stream-caret::after {
  content: "";
  width: 7px;
  height: 14px;
  background: var(--purple-bright);
  animation: stream-caret-blink 0.8s steps(1) infinite;
}

.stop-generation-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  min-width: 78px;
  height: 42px;
  padding: 0 14px;
  border: 1px solid rgb(255 116 116 / 42%);
  border-radius: 10px;
  background: rgb(127 32 46 / 30%);
  color: rgb(255 208 211);
  font-weight: 700;
}

.stop-generation-button:hover {
  border-color: rgb(255 116 116 / 72%);
  background: rgb(153 38 53 / 42%);
}

.stop-generation-button > span:first-child {
  font-size: 0.63rem;
}

@keyframes stream-caret-blink {
  0%,
  48% {
    opacity: 1;
  }

  49%,
  100% {
    opacity: 0;
  }
}
'''

STYLES.write_text(styles + "\n", encoding="utf-8", newline="\n")


print(f"UPDATED {CHAT}")
print(f"UPDATED {CARGO}")
print(f"UPDATED {LIB}")
print(f"UPDATED {COMMAND_TESTS}")
print(f"UPDATED {CONTRACTS}")
print(f"UPDATED {CLIENT}")
print(f"UPDATED {APP}")
print(f"UPDATED {STYLES}")
print("PHASE_C4_3_STREAMING_WRITTEN")
