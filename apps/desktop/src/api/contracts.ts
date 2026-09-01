export type CommandErrorCode =
  | "invalid_request"
  | "not_found"
  | "access_denied"
  | "conflict"
  | "storage_failure"
  | "integrity_failure";

export interface CommandError {
  code: CommandErrorCode;
  details?: string;
}

export interface WorkspaceDto {
  id: string;
  name: string;
  createdAtUtc: string;
  archivedAtUtc: string | null;
}

export type SourceKind =
  "plain_text" | "markdown" | "json" | "source_code" | "build_log" | "pdf_text";

export type DocumentVersionStatus = "processing" | "ready" | "failed";

export interface DocumentVersionDto {
  id: string;
  contentSha256: string;
  parserId: string;
  parserVersion: string;
  status: DocumentVersionStatus;
  createdAtUtc: string;
  failureCode: string | null;
  failureMessage: string | null;
}

export interface SourceDto {
  id: string;
  workspaceId: string;
  kind: SourceKind;
  displayName: string;
  canonicalLocator: string;
  createdAtUtc: string;
  lastSeenAtUtc: string;
  versions: DocumentVersionDto[];
}

export interface IngestFileResultDto {
  path: string;
  status: "succeeded" | "failed";
  sourceId: string | null;
  documentVersionId: string | null;
  error: CommandError | null;
}

export type SourceSpanDto =
  | { kind: "lines"; startLine: number; endLine: number }
  | { kind: "pdf_page"; page: number; startChar: number; endChar: number };

export interface SearchRequestDto {
  workspaceId: string;
  query: string;
  sourceKinds: SourceKind[];
  limit: number;
}

export interface SearchHitDto {
  blockId: string;
  documentVersionId: string;
  sourceId: string;
  score: number;
  snippet: string;
  span: SourceSpanDto;
}

export interface CitationDto {
  sourceId: string;
  displayName: string;
  canonicalLocator: string;
  contentSha256: string;
  span: SourceSpanDto;
  excerpt: string;
}

export interface AuditEventDto {
  id: string;
  workspaceId: string;
  sequence: number;
  eventType: string;
  actor: "user" | "system" | "maintenance";
  occurredAtUtc: string;
  eventHash: string;
}

export type IntegrityFailureKind =
  | "invalid_payload"
  | "previous_hash_mismatch"
  | "event_hash_mismatch"
  | "workspace_mismatch";

export interface IntegrityVerificationDto {
  workspaceId: string;
  eventCount: number;
  isValid: boolean;
  firstFailure: { kind: IntegrityFailureKind; sequence: number } | null;
}

export type KhojRuntimeState = "Stopped" | "Starting" | "Ready" | "Failed" | "Stopping";

export interface KhojRuntimeStatusDto {
  state: KhojRuntimeState;
  pid: number | null;
  endpoint: string;
  started_at: string | null;
  last_error: string | null;
}

export type LocalModelRuntimeState =
  "Stopped" | "Starting" | "Ready" | "Failed" | "Stopping";

export interface LocalModelRuntimeStatusDto {
  state: LocalModelRuntimeState;
  pid: number | null;
  endpoint: string;
  model_id: string;
  model_path: string;
  started_at: string | null;
  last_error: string | null;
}

export type RetrievalRuntimeRole = "embedding" | "reranker";

export type RetrievalRuntimeState =
  "Stopped" | "Starting" | "Ready" | "Failed" | "Stopping";

export interface RetrievalRuntimeStatusDto {
  role: RetrievalRuntimeRole;
  state: RetrievalRuntimeState;
  pid: number | null;
  endpoint: string;
  model_id: string;
  model_path: string;
  started_at: string | null;
  last_error: string | null;
}

export type ChatKnowledgeMode = "general" | "ask_workspace" | "evidence_only";

export interface ChatSendRequestDto {
  query: string;
  conversation_id: string | null;
  create_new: boolean;
  mode: ChatKnowledgeMode;
  workspace_id: string | null;
  selected_source_ids: string[];
}

export interface RagEvidenceDto {
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

export type RagStrategy = "fts_only" | "vector_only" | "hybrid";

export interface RagRetrievalDto {
  strategy: RagStrategy;
  ftsUsed: boolean;
  vectorUsed: boolean;
  rerankerUsed: boolean;
  candidateCount: number;
  selectedCount: number;
  evidence: RagEvidenceDto[];
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
    workspace_id: string | null;
    rag_by_turn: Record<string, RagRetrievalDto>;
  };
}

export type ChatStreamEventKind =
  | "metadata"
  | "references"
  | "rag-metadata"
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
