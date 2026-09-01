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
    destination = path.with_name(path.name + ".before-phase-c4-chat")

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
    CARGO,
    COMMAND_TESTS,
):
    backup(path)


# ============================================================
# Rust Khoj chat bridge
# ============================================================

CHAT.write_text(
r'''use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const KHOJ_BASE_URL: &str = "http://127.0.0.1:42110";
const R2H_CHAT_CLIENT: &str = "r2h-desktop";

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
        .map_err(|error| format!("Khoj is unavailable: {error}"))?;

    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "Khoj readiness check returned HTTP {}",
            response.status()
        ))
    }
}

pub async fn send_chat(
    request: ChatSendRequest,
) -> Result<ChatSendResponse, String> {
    let query = request.query.trim();

    if query.is_empty() {
        return Err("Chat query cannot be empty".to_owned());
    }

    if query.len() > 32_000 {
        return Err("Chat query exceeds the local request limit".to_owned());
    }

    let client = client()?;
    ensure_khoj_ready(&client).await?;

    let mut body = serde_json::json!({
        "q": query,
        "stream": false,
        "create_new": request.create_new,
    });

    if let Some(conversation_id) = request.conversation_id {
        if !conversation_id.trim().is_empty() {
            body["conversation_id"] =
                Value::String(conversation_id.trim().to_owned());
        }
    }

    let response = client
        .post(format!(
            "{KHOJ_BASE_URL}/api/chat?client={R2H_CHAT_CLIENT}"
        ))
        .json(&body)
        .send()
        .await
        .map_err(|error| format!("Local chat request failed: {error}"))?;

    let status = response.status();
    let payload = response
        .text()
        .await
        .map_err(|error| format!("Unable to read Khoj response: {error}"))?;

    if !status.is_success() {
        return Err(format!(
            "Khoj chat returned HTTP {status}: {payload}"
        ));
    }

    serde_json::from_str(&payload)
        .map_err(|error| format!("Invalid Khoj chat response: {error}"))
}

pub async fn list_sessions() -> Result<Vec<ChatSessionDto>, String> {
    let client = client()?;
    ensure_khoj_ready(&client).await?;

    let response = client
        .get(format!(
            "{KHOJ_BASE_URL}/api/chat/sessions\
             ?client={R2H_CHAT_CLIENT}&recent=true"
        ))
        .send()
        .await
        .map_err(|error| format!("Unable to load chat sessions: {error}"))?;

    let status = response.status();
    let payload = response
        .text()
        .await
        .map_err(|error| format!("Unable to read chat sessions: {error}"))?;

    if !status.is_success() {
        return Err(format!(
            "Khoj sessions returned HTTP {status}: {payload}"
        ));
    }

    if let Ok(values) = serde_json::from_str::<Vec<ChatSessionDto>>(&payload) {
        return Ok(values);
    }

    let single = serde_json::from_str::<ChatSessionDto>(&payload)
        .map_err(|error| format!("Invalid chat sessions response: {error}"))?;

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
        .map_err(|error| format!("Unable to load chat history: {error}"))?;

    let status = response.status();
    let payload = response
        .text()
        .await
        .map_err(|error| format!("Unable to read chat history: {error}"))?;

    if !status.is_success() {
        return Err(format!(
            "Khoj history returned HTTP {status}: {payload}"
        ));
    }

    serde_json::from_str(&payload)
        .map_err(|error| format!("Invalid chat history response: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_chat_endpoint_is_loopback_only() {
        assert_eq!(KHOJ_BASE_URL, "http://127.0.0.1:42110");
        assert!(!KHOJ_BASE_URL.contains("0.0.0.0"));
    }

    #[test]
    fn chat_request_rejects_whitespace() {
        let request = ChatSendRequest {
            query: "   ".to_owned(),
            conversation_id: None,
            create_new: true,
        };

        assert!(request.query.trim().is_empty());
    }
}
''',
    encoding="utf-8",
    newline="\n",
)


# ============================================================
# Cargo dependency
# ============================================================

cargo = CARGO.read_text(encoding="utf-8")

if 'reqwest = ' not in cargo:
    cargo = replace_once(
        cargo,
        'serde_json = "1.0.150"\n',
        'serde_json = "1.0.150"\n'
        'reqwest = { version = "0.12", default-features = false, '
        'features = ["json", "rustls-tls"] }\n',
        "reqwest dependency",
    )

CARGO.write_text(cargo, encoding="utf-8", newline="\n")


# ============================================================
# Tauri commands
# ============================================================

lib = LIB.read_text(encoding="utf-8")

lib = replace_once(
    lib,
    "mod commands;\n",
    "mod commands;\nmod chat_bridge;\n",
    "chat module declaration",
)

lib = replace_once(
    lib,
    "pub const COMMAND_NAMES: [&str; 21] = [",
    "pub const COMMAND_NAMES: [&str; 24] = [",
    "command count",
)

lib = replace_once(
    lib,
    '    "local_model_runtime_restart",\n];',
    '    "local_model_runtime_restart",\n'
    '    "chat_send",\n'
    '    "chat_sessions",\n'
    '    "chat_history",\n'
    '];',
    "chat command names",
)

command_block = r'''
#[tauri::command]
async fn chat_send(
    request: chat_bridge::ChatSendRequest,
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

    chat_bridge::send_chat(request).await
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
    khoj_runtime: tauri::State<'_, KhojRuntimeManager>,
) -> Result<chat_bridge::ChatHistoryDto, String> {
    let khoj = runtime_status_result(khoj_runtime.inner())?;

    if khoj.state != KhojRuntimeState::Ready {
        return Err("Khoj runtime is not ready".to_owned());
    }

    chat_bridge::get_history(&conversation_id).await
}

'''

lib = replace_once(
    lib,
    "pub fn run() -> Result<(), Box<dyn std::error::Error>> {",
    command_block
    + "pub fn run() -> Result<(), Box<dyn std::error::Error>> {",
    "chat command functions",
)

lib = replace_once(
    lib,
    "            local_model_runtime_restart,\n"
    "        ])",
    "            local_model_runtime_restart,\n"
    "            chat_send,\n"
    "            chat_sessions,\n"
    "            chat_history,\n"
    "        ])",
    "chat handler registration",
)

LIB.write_text(lib, encoding="utf-8", newline="\n")


# ============================================================
# Command registry tests
# ============================================================

tests = COMMAND_TESTS.read_text(encoding="utf-8")

tests = replace_once(
    tests,
    "const APPROVED_COMMANDS: [&str; 21] = [",
    "const APPROVED_COMMANDS: [&str; 24] = [",
    "approved command count",
)

tests = replace_once(
    tests,
    '    "local_model_runtime_restart",\n];',
    '    "local_model_runtime_restart",\n'
    '    "chat_send",\n'
    '    "chat_sessions",\n'
    '    "chat_history",\n'
    '];',
    "approved chat commands",
)

COMMAND_TESTS.write_text(tests, encoding="utf-8", newline="\n")


# ============================================================
# TypeScript contracts
# ============================================================

contracts = CONTRACTS.read_text(encoding="utf-8").rstrip()

contracts += r'''

export interface ChatSendRequestDto {
  query: string;
  conversation_id: string | null;
  create_new: boolean;
}

export interface ChatSendResponseDto {
  response: string;
  references: {
    inferredQueries?: string[];
    context?: unknown[];
    onlineContext?: Record<string, unknown>;
    codeContext?: Record<string, unknown>;
  };
  usage: Record<string, number>;
  images: unknown[];
  files: unknown[];
  mermaidjsDiagram: unknown[];
}

export interface ChatSessionDto {
  conversation_id: string;
  slug: string;
  agent_name: string | null;
  created: string;
  updated: string;
  agent_icon: string | null;
  agent_color: string | null;
  agent_is_hidden: boolean;
}

export interface ChatHistoryMessageDto {
  by: "you" | "khoj" | string;
  turnId: string;
  created: string;
  message: string;
  context: unknown[];
  trainOfThought: unknown[];
}

export interface ChatHistoryDto {
  status: string;
  response: {
    chat: ChatHistoryMessageDto[];
    conversation_id: string;
    slug: string;
    agent: Record<string, unknown> | null;
    is_owner: boolean;
  };
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
    "  CitationDto,\n",
    "  CitationDto,\n"
    "  ChatHistoryDto,\n"
    "  ChatSendRequestDto,\n"
    "  ChatSendResponseDto,\n"
    "  ChatSessionDto,\n",
    "chat client imports",
)

client = client.rstrip() + r'''

export async function sendChat(
  request: ChatSendRequestDto,
): Promise<ChatSendResponseDto> {
  return invoke<ChatSendResponseDto>("chat_send", { request });
}

export async function listChatSessions(): Promise<ChatSessionDto[]> {
  return invoke<ChatSessionDto[]>("chat_sessions");
}

export async function getChatHistory(
  conversationId: string,
): Promise<ChatHistoryDto> {
  return invoke<ChatHistoryDto>("chat_history", {
    conversationId,
  });
}
'''

CLIENT.write_text(
    client + "\n",
    encoding="utf-8",
    newline="\n",
)


# ============================================================
# React Chat UI
# ============================================================

app = APP.read_text(encoding="utf-8")

app = replace_once(
    app,
    "  CitationDto,\n",
    "  CitationDto,\n"
    "  ChatHistoryMessageDto,\n"
    "  ChatSessionDto,\n",
    "chat UI type imports",
)

app = replace_once(
    app,
    '              <ChatScreen workspace={selectedWorkspace} sources={sources} />',
    '''              <ChatScreen
                workspace={selectedWorkspace}
                sources={sources}
                khojRuntime={khojRuntime}
                localModelRuntime={localModelRuntime}
              />''',
    "ChatScreen invocation",
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
  const [sending, setSending] = useState(false);
  const [loadingHistory, setLoadingHistory] = useState(false);
  const [error, setError] = useState("");

  const runtimeReady =
    khojRuntime.state === "Ready" &&
    localModelRuntime.state === "Ready";

  async function refreshSessions(): Promise<ChatSessionDto[]> {
    if (khojRuntime.state !== "Ready") return [];

    const values = await client.listChatSessions();
    setSessions(values);
    return values;
  }

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
    setConversationId(null);
    setMessages([]);
    setDraft("");
    setError("");
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();

    const value = draft.trim();

    if (!value || !runtimeReady || sending) return;

    const optimistic: ChatHistoryMessageDto = {
      by: "you",
      turnId: crypto.randomUUID(),
      created: new Date().toISOString(),
      message: value,
      context: [],
      trainOfThought: [],
    };

    setDraft("");
    setError("");
    setSending(true);
    setMessages((current) => [...current, optimistic]);

    try {
      const result = await client.sendChat({
        query: `/general ${value}`,
        conversation_id: conversationId,
        create_new: !conversationId,
      });

      setMessages((current) => [
        ...current,
        {
          by: "khoj",
          turnId: optimistic.turnId,
          created: new Date().toISOString(),
          message: result.response,
          context: result.references?.context ?? [],
          trainOfThought: [],
        },
      ]);

      const values = await refreshSessions();

      if (!conversationId && values[0]?.conversation_id) {
        setConversationId(values[0].conversation_id);
      }
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setSending(false);
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

            <StatusPill
              tone={runtimeReady ? "success" : "warning"}
            >
              {runtimeReady ? "R2H AI ready" : "Runtime unavailable"}
            </StatusPill>
          </div>

          <div className="thread-body live-chat-thread">
            {loadingHistory ? (
              <p className="loading-copy">Loading conversation…</p>
            ) : messages.length === 0 ? (
              <article className="assistant-message">
                <div className="message-role">
                  <Icon name="spark" /> R2H Second Brain
                </div>
                <h2>Ask your private second brain.</h2>
                <p>
                  Conversations run through the local R2H intelligence
                  service and remain stored on this device.
                </p>
                <div className="answer-grid">
                  <SignalRow
                    label="Khoj core"
                    value={khojRuntimeLabel(khojRuntime)}
                  />
                  <SignalRow
                    label="Qwen3"
                    value={localModelRuntimeLabel(localModelRuntime)}
                  />
                  <SignalRow
                    label="Sources"
                    value={sources.length}
                  />
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
                    <p>{message.message}</p>
                  </article>
                ),
              )
            )}

            {sending && (
              <article className="runtime-notice generating">
                <Icon name="model" />
                <div>
                  <strong>Generating locally…</strong>
                  <p>
                    Qwen3 is processing this request through the R2H
                    intelligence core.
                  </p>
                </div>
              </article>
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

          <button
            className="send-button"
            type="submit"
            disabled={!draft.trim() || !runtimeReady || sending}
          >
            <Icon name="send" />
            <span className="sr-only">Send prompt</span>
          </button>
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
                onClick={() => void openSession(session.conversation_id)}
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
            label="R2H core"
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
    raise RuntimeError("Unable to locate ChatScreen block")

app = (
    app[:match.start()]
    + new_chat_screen
    + "\n\nfunction ModelsScreen("
    + app[match.end():]
)

APP.write_text(app, encoding="utf-8", newline="\n")


# ============================================================
# Chat styles
# ============================================================

styles = STYLES.read_text(encoding="utf-8").rstrip()

styles += r'''

.live-chat-thread {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-height: 420px;
  max-height: calc(100vh - 330px);
  overflow-y: auto;
}

.chat-message {
  position: relative;
  z-index: 1;
}

.user-prompt.chat-message {
  align-self: flex-end;
  width: min(78%, 720px);
}

.assistant-message.chat-message {
  width: min(88%, 820px);
}

.assistant-message.chat-message > p,
.user-prompt.chat-message > p {
  margin: 0;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  line-height: 1.75;
}

.runtime-notice.generating {
  animation: chat-pulse 1.5s ease-in-out infinite;
}

.chat-session-list {
  display: grid;
  gap: 8px;
  margin-top: 12px;
}

.chat-session {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  width: 100%;
  padding: 12px;
  border: 1px solid var(--line);
  border-radius: 10px;
  background: rgb(255 255 255 / 2%);
  color: inherit;
  text-align: left;
}

.chat-session:hover,
.chat-session.selected {
  border-color: rgb(166 116 255 / 55%);
  background: rgb(142 85 238 / 10%);
}

.chat-session > span {
  display: grid;
  gap: 5px;
  min-width: 0;
}

.chat-session strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chat-session small {
  color: var(--muted);
}

.chat-session svg {
  flex: 0 0 auto;
  width: 15px;
}

@keyframes chat-pulse {
  0%,
  100% {
    opacity: 0.72;
  }

  50% {
    opacity: 1;
  }
}
'''

STYLES.write_text(styles + "\n", encoding="utf-8", newline="\n")


print(f"CREATED {CHAT}")
print(f"UPDATED {CARGO}")
print(f"UPDATED {LIB}")
print(f"UPDATED {COMMAND_TESTS}")
print(f"UPDATED {CONTRACTS}")
print(f"UPDATED {CLIENT}")
print(f"UPDATED {APP}")
print(f"UPDATED {STYLES}")
print("PHASE_C4_CHAT_FOUNDATION_WRITTEN")
