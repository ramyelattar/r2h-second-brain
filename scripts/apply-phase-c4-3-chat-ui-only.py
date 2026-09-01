from pathlib import Path

ROOT = Path(r"E:\Projects\r2h-second-brain")
APP = ROOT / "apps" / "desktop" / "src" / "App.tsx"

text = APP.read_text(encoding="utf-8")

backup = APP.with_name("App.tsx.before-c4-3-chat-ui-only")
backup.write_text(text, encoding="utf-8", newline="\n")


# ============================================================
# Imports
# ============================================================

old_react_import = 'import { useEffect, useMemo, useState } from "react";'
new_react_import = (
    'import { useEffect, useMemo, useRef, useState } from "react";\n'
    'import { listen } from "@tauri-apps/api/event";'
)

if old_react_import in text:
    text = text.replace(old_react_import, new_react_import, 1)
elif 'import { useEffect, useMemo, useRef, useState } from "react";' not in text:
    raise RuntimeError("Unable to update React imports")

if "  ChatStreamEventDto,\n" not in text:
    marker = "  ChatSessionDto,\n"

    if marker not in text:
        raise RuntimeError("Unable to locate ChatSessionDto import")

    text = text.replace(
        marker,
        marker + "  ChatStreamEventDto,\n",
        1,
    )


# ============================================================
# Locate current ChatScreen
# ============================================================

start = text.find("function ChatScreen({")
end = text.find("\nfunction ModelsScreen(", start)

if start < 0:
    raise RuntimeError("Unable to locate ChatScreen start")

if end < 0:
    raise RuntimeError("Unable to locate ChatScreen end")


# ============================================================
# C4.3 Chat UI
# ============================================================

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
  const conversationIdRef = useRef<string | null>(null);

  const runtimeReady =
    khojRuntime.state === "Ready" &&
    localModelRuntime.state === "Ready";

  const sending = activeRequestId !== null;

  useEffect(() => {
    conversationIdRef.current = conversationId;
  }, [conversationId]);

  async function refreshSessions(): Promise<ChatSessionDto[]> {
    if (khojRuntime.state !== "Ready") return [];

    const values = await client.listChatSessions();
    setSessions(values);

    return values;
  }

  async function reconcileConversation(id?: string | null) {
    let target = id ?? conversationIdRef.current;
    const values = await refreshSessions();

    if (!target) {
      target = values[0]?.conversation_id ?? null;
    }

    if (!target) return;

    const history = await client.getChatHistory(target);

    conversationIdRef.current = history.response.conversation_id;
    setConversationId(history.response.conversation_id);
    setMessages(history.response.chat);
  }

  useEffect(() => {
    const element = threadRef.current;

    if (!element) return;

    element.scrollTo({
      top: element.scrollHeight,
      behavior: "smooth",
    });
  }, [messages, streamStatus]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listen<ChatStreamEventDto>("r2h-chat-stream", (event) => {
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
            conversationIdRef.current = data.conversationId;
            setConversationId(data.conversationId);
          }

          if (data.turnId) {
            assistantTurnRef.current = data.turnId;
          }

          break;
        }

        case "references":
          break;

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
              message.by !== "you" && message.turnId === turnId
                ? {
                    ...message,
                    message: message.message + token,
                  }
                : message,
            ),
          );

          break;
        }

        case "response-end":
        case "usage":
        case "protocol":
          break;

        case "done":
        case "cancelled": {
          const data = payload.data as {
            conversationId?: string;
          };

          activeRequestRef.current = null;
          assistantTurnRef.current = null;

          setActiveRequestId(null);
          setStreamStatus("");

          void reconcileConversation(data.conversationId).catch(
            (reason: unknown) => {
              setError(errorMessage(reason));
            },
          );

          break;
        }

        case "error":
          activeRequestRef.current = null;
          assistantTurnRef.current = null;

          setActiveRequestId(null);
          setStreamStatus("");

          setError(
            typeof payload.data === "string"
              ? payload.data
              : "Local generation failed.",
          );

          break;
      }
    }).then((dispose) => {
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
  }, []);

  useEffect(() => {
    if (khojRuntime.state !== "Ready") {
      setSessions([]);
      return;
    }

    let active = true;

    void client
      .listChatSessions()
      .then((values) => {
        if (active) {
          setSessions(values);
        }
      })
      .catch((reason: unknown) => {
        if (active) {
          setError(errorMessage(reason));
        }
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

      conversationIdRef.current = history.response.conversation_id;
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

    conversationIdRef.current = null;
    assistantTurnRef.current = null;

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

    setDraft("");
    setError("");
    setStreamStatus("Preparing local generation…");

    activeRequestRef.current = requestId;
    assistantTurnRef.current = null;

    setActiveRequestId(requestId);

    setMessages((current) => [
      ...current,
      {
        by: "you",
        turnId: crypto.randomUUID(),
        created: new Date().toISOString(),
        message: value,
        context: [],
        trainOfThought: [],
      },
    ]);

    try {
      await client.startChatStream(requestId, {
        query: `/general ${value}`,
        conversation_id: conversationIdRef.current,
        create_new: !conversationIdRef.current,
      });
    } catch (reason) {
      activeRequestRef.current = null;
      assistantTurnRef.current = null;

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
        activeRequestRef.current = null;
        assistantTurnRef.current = null;

        setActiveRequestId(null);
        setStreamStatus("");
        setError("The active local request was not found.");
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
              aria-label="Send prompt"
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

          <SignalRow
            label="Network"
            value="Loopback only"
          />

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

text = text[:start] + new_chat_screen + text[end:]


# ============================================================
# Visible branding outside ChatScreen
# ============================================================

text = text.replace(
    "Khoj · ${khojRuntimeLabel(khojRuntime)}",
    "R2H Core · ${khojRuntimeLabel(khojRuntime)}",
)

text = text.replace(
    'label="Khoj service"',
    'label="R2H Intelligence Core"',
)


# ============================================================
# Verification before write
# ============================================================

required = [
    'import { listen } from "@tauri-apps/api/event";',
    "ChatStreamEventDto",
    "activeRequestRef",
    'listen<ChatStreamEventDto>("r2h-chat-stream"',
    "client.startChatStream",
    "client.cancelChatStream",
    "stop-generation-button",
    'label="R2H Intelligence Core"',
]

missing = [item for item in required if item not in text]

if missing:
    raise RuntimeError(
        "C4.3 Chat UI verification failed. Missing: "
        + ", ".join(missing)
    )

if "const [sending, setSending]" in text[start:start + len(new_chat_screen) + 500]:
    raise RuntimeError("Old C4.2 sending state remains in ChatScreen")

if "await client.sendChat(" in text[start:start + len(new_chat_screen) + 500]:
    raise RuntimeError("Old non-streaming sendChat call remains in ChatScreen")


APP.write_text(text, encoding="utf-8", newline="\n")

print("C4_3_CHAT_UI_REPLACED")
print(f"BACKUP={backup}")
