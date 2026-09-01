from __future__ import annotations

from pathlib import Path


ROOT = Path(r"E:\Projects\r2h-second-brain")
APP = ROOT / "apps" / "desktop" / "src" / "App.tsx"
CONTRACTS = ROOT / "apps" / "desktop" / "src" / "api" / "contracts.ts"
STYLES = ROOT / "apps" / "desktop" / "src" / "styles.css"
LIB = ROOT / "apps" / "desktop" / "src-tauri" / "src" / "lib.rs"

FILES = [APP, CONTRACTS, STYLES, LIB]

for path in FILES:
    if not path.is_file():
        raise RuntimeError(f"Required file is missing: {path}")


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8-sig")


def write(path: Path, value: str) -> None:
    backup = path.with_name(path.name + ".before-rag-1b")

    if not backup.exists():
        backup.write_text(
            read(path),
            encoding="utf-8",
            newline="\n",
        )

    path.write_text(
        value,
        encoding="utf-8",
        newline="\n",
    )


def replace_once(
    text: str,
    old: str,
    new: str,
    label: str,
) -> str:
    count = text.count(old)

    if count != 1:
        raise RuntimeError(
            f"{label}: expected one match, found {count}"
        )

    return text.replace(old, new, 1)


# ============================================================
# TypeScript contracts
# ============================================================

contracts = read(CONTRACTS)

if "export interface RagEvidenceDto" not in contracts:
    marker = '''export interface ChatSendResponseDto {'''

    addition = '''export interface RagEvidenceDto {
  citationNumber: number;
  blockId: string;
  documentVersionId: string;
  sourceId: string;
  displayName: string;
  canonicalLocator: string;
  contentSha256: string;
  span: SourceSpanDto;
  excerpt: string;
  score: number;
}

export interface RagRetrievalDto {
  strategy: "fts_only" | "hybrid";
  ftsUsed: boolean;
  vectorUsed: boolean;
  rerankerUsed: boolean;
  candidateCount: number;
  selectedCount: number;
  evidence: RagEvidenceDto[];
}

'''

    contracts = replace_once(
        contracts,
        marker,
        addition + marker,
        "add RAG DTO contracts",
    )

contracts = replace_once(
    contracts,
    '''  | "references"
  | "status"''',
    '''  | "references"
  | "rag-metadata"
  | "status"''',
    "add rag-metadata event kind",
)

write(CONTRACTS, contracts)


# ============================================================
# Rust: mark local insufficient-evidence completion
# ============================================================

lib = read(LIB)

lib = replace_once(
    lib,
    '''                data: serde_json::json!({
                    "conversationId": prepared.request.conversation_id,
                    "turnId": null
                }),''',
    '''                data: serde_json::json!({
                    "conversationId": prepared.request.conversation_id,
                    "turnId": null,
                    "localOnly": true
                }),''',
    "mark local-only insufficient result",
)

write(LIB, lib)


# ============================================================
# App imports
# ============================================================

app = read(APP)

app = replace_once(
    app,
    '''  LocalModelRuntimeStatusDto,
  SearchHitDto,''',
    '''  LocalModelRuntimeStatusDto,
  RagEvidenceDto,
  RagRetrievalDto,
  SearchHitDto,''',
    "import RAG DTO types",
)


# ============================================================
# Shared RAG display helpers
# ============================================================

if "function RagMessageText(" not in app:
    marker = '''function ChatScreen({'''

    helpers = r'''function RagMessageText({
  text,
  evidence,
  onOpenEvidence,
}: {
  text: string;
  evidence: RagEvidenceDto[];
  onOpenEvidence: (evidence: RagEvidenceDto) => void;
}) {
  const validNumbers = new Set(
    evidence.map((item) => item.citationNumber),
  );

  const parts = text.split(/(\[\d+\])/g);

  return (
    <>
      {parts.map((part, index) => {
        const match = /^\[(\d+)\]$/.exec(part);

        if (!match) {
          return (
            <span key={`text-${index}`}>
              {part}
            </span>
          );
        }

        const citationNumber = Number(match[1]);

        if (!validNumbers.has(citationNumber)) {
          return (
            <span
              className="invalid-citation"
              key={`invalid-${index}`}
              title="Citation was not present in retrieved evidence"
            >
              {part}
            </span>
          );
        }

        const item = evidence.find(
          (candidate) =>
            candidate.citationNumber === citationNumber,
        );

        if (!item) return part;

        return (
          <button
            type="button"
            className="inline-citation"
            key={`citation-${citationNumber}-${index}`}
            onClick={() => onOpenEvidence(item)}
            aria-label={`Open citation ${citationNumber}: ${item.displayName}`}
          >
            [{citationNumber}]
          </button>
        );
      })}
    </>
  );
}

function RagSourceCards({
  retrieval,
  onOpenEvidence,
}: {
  retrieval: RagRetrievalDto;
  onOpenEvidence: (evidence: RagEvidenceDto) => void;
}) {
  return (
    <section className="rag-evidence-section">
      <div className="rag-evidence-heading">
        <div>
          <span>Retrieved evidence</span>
          <strong>
            {retrieval.selectedCount} source
            {retrieval.selectedCount === 1 ? "" : "s"}
          </strong>
        </div>

        <div className="rag-layer-badges">
          <span className={retrieval.ftsUsed ? "active" : ""}>
            FTS
          </span>
          <span className={retrieval.vectorUsed ? "active" : ""}>
            Vector
          </span>
          <span className={retrieval.rerankerUsed ? "active" : ""}>
            Reranker
          </span>
        </div>
      </div>

      <div className="rag-source-list">
        {retrieval.evidence.map((item) => (
          <article
            className="rag-source-card"
            key={`${item.blockId}-${item.citationNumber}`}
          >
            <div className="rag-source-number">
              [{item.citationNumber}]
            </div>

            <div className="rag-source-body">
              <div className="rag-source-meta">
                <strong>{item.displayName}</strong>
                <span>{spanLabel(item.span)}</span>
                <span>
                  {Math.round(item.score * 100)}% relevance
                </span>
              </div>

              <p>{item.excerpt}</p>

              <div className="rag-source-actions">
                <button
                  type="button"
                  className="text-button"
                  onClick={() => onOpenEvidence(item)}
                >
                  Preview evidence
                </button>

                <code title={item.contentSha256}>
                  {hashPrefix(item.contentSha256)}
                </code>
              </div>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}

'''

    app = replace_once(
        app,
        marker,
        helpers + marker,
        "add RAG display helpers",
    )


# ============================================================
# Chat state
# ============================================================

app = replace_once(
    app,
    '''  const [loadingHistory, setLoadingHistory] = useState(false);
  const [error, setError] = useState("");''',
    '''  const [loadingHistory, setLoadingHistory] = useState(false);
  const [error, setError] = useState("");
  const [selectedSourceIds, setSelectedSourceIds] =
    useState<string[]>([]);
  const [ragByTurn, setRagByTurn] =
    useState<Record<string, RagRetrievalDto>>({});
  const [previewEvidence, setPreviewEvidence] =
    useState<RagEvidenceDto | null>(null);''',
    "add RAG UI state",
)

app = replace_once(
    app,
    '''  const assistantTurnRef = useRef<string | null>(null);
  const conversationIdRef = useRef<string | null>(null);''',
    '''  const assistantTurnRef = useRef<string | null>(null);
  const conversationIdRef = useRef<string | null>(null);
  const pendingRagRef = useRef<RagRetrievalDto | null>(null);''',
    "add pending RAG ref",
)


# ============================================================
# Reset invalid selected sources when workspace source list changes
# ============================================================

app = replace_once(
    app,
    '''  useEffect(() => {
    localStorage.setItem("r2h.chat-mode.v1", chatMode);
  }, [chatMode]);''',
    '''  useEffect(() => {
    localStorage.setItem("r2h.chat-mode.v1", chatMode);
  }, [chatMode]);

  useEffect(() => {
    const available = new Set(
      sources.map((source) => source.id),
    );

    setSelectedSourceIds((current) =>
      current.filter((sourceId) => available.has(sourceId)),
    );
  }, [sources]);

  useEffect(() => {
    if (chatMode === "general") {
      setSelectedSourceIds([]);
    }
  }, [chatMode]);''',
    "add source-selection lifecycle",
)


# ============================================================
# Stream event: RAG metadata
# ============================================================

app = replace_once(
    app,
    '''        case "references":
          break;

        case "status":''',
    '''        case "references":
          break;

        case "rag-metadata": {
          const retrieval = payload.data as RagRetrievalDto;

          pendingRagRef.current = retrieval;

          setStreamStatus(
            retrieval.selectedCount === 0
              ? "No matching workspace evidence"
              : `Reviewing ${retrieval.selectedCount} evidence item${
                  retrieval.selectedCount === 1 ? "" : "s"
                }…`,
          );

          break;
        }

        case "status":''',
    "handle rag-metadata event",
)


# ============================================================
# Associate evidence metadata with assistant turn
# ============================================================

app = replace_once(
    app,
    '''          setMessages((current) => [
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

          break;''',
    '''          setMessages((current) => [
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

          if (pendingRagRef.current) {
            const retrieval = pendingRagRef.current;

            setRagByTurn((current) => ({
              ...current,
              [turnId]: retrieval,
            }));
          }

          break;''',
    "attach RAG metadata to assistant turn",
)


# ============================================================
# Done / cancellation cleanup
# ============================================================

app = replace_once(
    app,
    '''        case "done":
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
        }''',
    '''        case "done":
        case "cancelled": {
          const data = payload.data as {
            conversationId?: string;
            localOnly?: boolean;
          };

          const finishedTurnId = assistantTurnRef.current;
          const wasCancelled = payload.kind === "cancelled";

          activeRequestRef.current = null;
          assistantTurnRef.current = null;
          pendingRagRef.current = null;

          setActiveRequestId(null);
          setStreamStatus("");

          if (wasCancelled && finishedTurnId) {
            setMessages((current) =>
              current.flatMap((message) => {
                if (
                  message.by === "you" ||
                  message.turnId !== finishedTurnId
                ) {
                  return [message];
                }

                if (!message.message.trim()) {
                  return [];
                }

                return [
                  {
                    ...message,
                    message: `${message.message}\n\nGeneration stopped.`,
                  },
                ];
              }),
            );
          }

          if (!data.localOnly) {
            void reconcileConversation(data.conversationId).catch(
              (reason: unknown) => {
                setError(errorMessage(reason));
              },
            );
          }

          break;
        }''',
    "clean cancelled and local-only completion",
)


# ============================================================
# Reset state for session/new conversation
# ============================================================

app = replace_once(
    app,
    '''      setConversationId(history.response.conversation_id);
      setMessages(history.response.chat);''',
    '''      setConversationId(history.response.conversation_id);
      setMessages(history.response.chat);
      setRagByTurn({});
      setPreviewEvidence(null);
      pendingRagRef.current = null;''',
    "reset RAG state when opening session",
)

app = replace_once(
    app,
    '''    setConversationId(null);
    setMessages([]);
    setDraft("");
    setError("");
    setStreamStatus("");''',
    '''    setConversationId(null);
    setMessages([]);
    setDraft("");
    setError("");
    setStreamStatus("");
    setSelectedSourceIds([]);
    setRagByTurn({});
    setPreviewEvidence(null);
    pendingRagRef.current = null;''',
    "reset RAG state for new conversation",
)


# ============================================================
# Send selected source IDs
# ============================================================

app = replace_once(
    app,
    '''        selected_source_ids: [],
      });''',
    '''        selected_source_ids:
          chatMode === "general" ? [] : selectedSourceIds,
      });''',
    "send selected source IDs",
)


# ============================================================
# Render assistant text and evidence cards
# ============================================================

app = replace_once(
    app,
    '''                    <p>
                      {message.message || (
                        <span className="stream-caret">
                          Generating…
                        </span>
                      )}
                    </p>
                  </article>''',
    '''                    <p className="assistant-answer-text">
                      {message.message ? (
                        <RagMessageText
                          text={message.message}
                          evidence={
                            ragByTurn[message.turnId]?.evidence ?? []
                          }
                          onOpenEvidence={setPreviewEvidence}
                        />
                      ) : (
                        <span className="stream-caret">
                          Generating…
                        </span>
                      )}
                    </p>

                    {ragByTurn[message.turnId] && (
                      <RagSourceCards
                        retrieval={ragByTurn[message.turnId]}
                        onOpenEvidence={setPreviewEvidence}
                      />
                    )}
                  </article>''',
    "render RAG answer and source cards",
)


# ============================================================
# Source scope control
# ============================================================

app = replace_once(
    app,
    '''          <span className="chat-grounding-summary">
            {chatMode === "general"
              ? "No workspace content will be used"
              : `${workspace?.name ?? "No workspace"} · FTS`}
          </span>
        </div>

        <form className="chat-composer"''',
    '''          <span className="chat-grounding-summary">
            {chatMode === "general"
              ? "No workspace content will be used"
              : `${workspace?.name ?? "No workspace"} · FTS · ${
                  selectedSourceIds.length === 0
                    ? "All sources"
                    : `${selectedSourceIds.length} selected`
                }`}
          </span>
        </div>

        {chatMode !== "general" && (
          <details className="chat-source-scope">
            <summary>
              <span>Source scope</span>
              <strong>
                {selectedSourceIds.length === 0
                  ? "All workspace sources"
                  : `${selectedSourceIds.length} selected`}
              </strong>
            </summary>

            <div className="chat-source-options">
              <button
                type="button"
                className="text-button"
                onClick={() => setSelectedSourceIds([])}
                disabled={
                  selectedSourceIds.length === 0 || sending
                }
              >
                Use all sources
              </button>

              {sources.length === 0 ? (
                <p>No indexed sources are available.</p>
              ) : (
                sources.map((source) => (
                  <label
                    className="chat-source-option"
                    key={source.id}
                  >
                    <input
                      type="checkbox"
                      checked={selectedSourceIds.includes(source.id)}
                      disabled={sending}
                      onChange={(event) => {
                        setSelectedSourceIds((current) =>
                          event.target.checked
                            ? [...current, source.id]
                            : current.filter(
                                (sourceId) =>
                                  sourceId !== source.id,
                              ),
                        );
                      }}
                    />

                    <span>
                      <strong>{source.displayName}</strong>
                      <small>{source.kind.replaceAll("_", " ")}</small>
                    </span>
                  </label>
                ))
              )}
            </div>
          </details>
        )}

        <form className="chat-composer"''',
    "add source scope selector",
)


# ============================================================
# Evidence preview modal
# ============================================================

app = replace_once(
    app,
    '''      <aside className="glass-panel context-drawer">''',
    '''      {previewEvidence && (
        <div
          className="evidence-preview-backdrop"
          role="presentation"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) {
              setPreviewEvidence(null);
            }
          }}
        >
          <section
            className="glass-panel evidence-preview-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="evidence-preview-title"
          >
            <div className="evidence-preview-header">
              <div>
                <span>
                  Citation [{previewEvidence.citationNumber}]
                </span>
                <h2 id="evidence-preview-title">
                  {previewEvidence.displayName}
                </h2>
                <p>{spanLabel(previewEvidence.span)}</p>
              </div>

              <button
                type="button"
                className="quiet-button"
                onClick={() => setPreviewEvidence(null)}
              >
                Close
              </button>
            </div>

            <pre className="evidence-preview-content">
              {previewEvidence.excerpt}
            </pre>

            <div className="evidence-preview-provenance">
              <span>Immutable source hash</span>
              <code>{previewEvidence.contentSha256}</code>
              <span>Stored locator</span>
              <code>{previewEvidence.canonicalLocator}</code>
            </div>
          </section>
        </div>
      )}

      <aside className="glass-panel context-drawer">''',
    "add evidence preview modal",
)

write(APP, app)


# ============================================================
# Styles
# ============================================================

styles = read(STYLES)

style_block = r'''

/* RAG evidence and grounded-chat controls */

.chat-source-scope {
  margin-top: 8px;
  border: 1px solid var(--line);
  border-radius: 12px;
  background: rgb(255 255 255 / 2%);
}

.chat-source-scope summary {
  min-height: 44px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 0 14px;
  cursor: pointer;
  color: var(--text-muted);
  font-size: 0.72rem;
}

.chat-source-scope summary strong {
  color: var(--white);
  font-size: 0.7rem;
}

.chat-source-options {
  max-height: 220px;
  display: grid;
  gap: 7px;
  padding: 10px 12px 12px;
  overflow-y: auto;
  border-top: 1px solid var(--line);
}

.chat-source-option {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  align-items: center;
  gap: 10px;
  padding: 9px 10px;
  border: 1px solid var(--line);
  border-radius: 9px;
  background: rgb(255 255 255 / 2%);
  cursor: pointer;
}

.chat-source-option input {
  accent-color: var(--purple);
}

.chat-source-option > span {
  min-width: 0;
  display: grid;
}

.chat-source-option strong {
  overflow: hidden;
  color: var(--white);
  font-size: 0.72rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chat-source-option small {
  color: var(--text-soft);
  font-size: 0.62rem;
  text-transform: capitalize;
}

.assistant-answer-text {
  white-space: pre-wrap;
}

.inline-citation {
  display: inline;
  margin: 0 2px;
  padding: 1px 5px;
  border: 1px solid var(--purple-line);
  border-radius: 6px;
  color: #d8b4fe;
  background: var(--purple-soft);
  font-size: 0.72rem;
  font-weight: 750;
  vertical-align: baseline;
}

.inline-citation:hover {
  border-color: var(--purple-bright);
  background: rgb(139 92 246 / 24%);
}

.invalid-citation {
  color: #fca5a5;
  text-decoration: underline dotted;
  text-underline-offset: 3px;
}

.rag-evidence-section {
  margin-top: 18px;
  padding-top: 15px;
  border-top: 1px solid var(--line);
}

.rag-evidence-heading {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  margin-bottom: 10px;
}

.rag-evidence-heading > div:first-child {
  display: grid;
}

.rag-evidence-heading span {
  color: var(--text-soft);
  font-size: 0.61rem;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.rag-evidence-heading strong {
  color: var(--white);
  font-size: 0.76rem;
}

.rag-layer-badges {
  display: flex;
  gap: 5px;
}

.rag-layer-badges span {
  padding: 3px 7px;
  border: 1px solid var(--line);
  border-radius: 999px;
  color: var(--text-soft);
  font-size: 0.55rem;
}

.rag-layer-badges span.active {
  border-color: var(--purple-line);
  color: #d8b4fe;
  background: var(--purple-soft);
}

.rag-source-list {
  display: grid;
  gap: 8px;
}

.rag-source-card {
  display: grid;
  grid-template-columns: 36px minmax(0, 1fr);
  gap: 10px;
  padding: 11px;
  border: 1px solid var(--line);
  border-radius: 11px;
  background: rgb(255 255 255 / 2%);
}

.rag-source-number {
  display: grid;
  width: 32px;
  height: 32px;
  place-items: center;
  border: 1px solid var(--purple-line);
  border-radius: 9px;
  color: #d8b4fe;
  background: var(--purple-soft);
  font-size: 0.68rem;
  font-weight: 800;
}

.rag-source-body {
  min-width: 0;
}

.rag-source-meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 5px 10px;
}

.rag-source-meta strong {
  max-width: 100%;
  overflow: hidden;
  color: var(--white);
  font-size: 0.72rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.rag-source-meta span {
  color: var(--text-soft);
  font-size: 0.61rem;
}

.rag-source-body p {
  display: -webkit-box;
  margin: 8px 0;
  overflow: hidden;
  font-size: 0.68rem;
  line-height: 1.55;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 3;
}

.rag-source-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.rag-source-actions code {
  color: var(--text-soft);
  font-size: 0.58rem;
}

.evidence-preview-backdrop {
  position: fixed;
  z-index: 100;
  inset: 0;
  display: grid;
  place-items: center;
  padding: 30px;
  background: rgb(0 0 0 / 68%);
  backdrop-filter: blur(8px);
}

.evidence-preview-modal {
  width: min(820px, 90vw);
  max-height: 86vh;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
  overflow: hidden;
  border-color: var(--purple-line);
  box-shadow: var(--shadow);
}

.evidence-preview-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 20px;
  padding: 18px;
  border-bottom: 1px solid var(--line);
}

.evidence-preview-header span {
  color: #c084fc;
  font-size: 0.63rem;
  font-weight: 750;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.evidence-preview-header h2 {
  margin-top: 5px;
}

.evidence-preview-header p {
  margin: 4px 0 0;
  font-size: 0.68rem;
}

.evidence-preview-content {
  margin: 0;
  padding: 20px;
  overflow: auto;
  color: rgb(245 247 251 / 82%);
  background: rgb(0 0 0 / 18%);
  font-family:
    "Cascadia Code",
    Consolas,
    monospace;
  font-size: 0.75rem;
  line-height: 1.7;
  white-space: pre-wrap;
  word-break: break-word;
}

.evidence-preview-provenance {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 7px 14px;
  padding: 14px 18px;
  border-top: 1px solid var(--line);
}

.evidence-preview-provenance span {
  color: var(--text-soft);
  font-size: 0.62rem;
}

.evidence-preview-provenance code {
  overflow: hidden;
  color: var(--text-muted);
  font-size: 0.61rem;
  text-overflow: ellipsis;
  white-space: nowrap;
}
'''

if "/* RAG evidence and grounded-chat controls */" not in styles:
    styles = styles.rstrip() + style_block + "\n"

write(STYLES, styles)

print("RAG_1B_PATCH_WRITTEN")
print("Modified:")
for path in [CONTRACTS, LIB, APP, STYLES]:
    print(f"  {path}")
