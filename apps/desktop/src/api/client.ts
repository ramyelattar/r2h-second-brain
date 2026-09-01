import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

import type {
  AuditEventDto,
  CitationDto,
  ChatHistoryDto,
  ChatSendRequestDto,
  ChatSendResponseDto,
  ChatSessionDto,
  ChatStreamStartResultDto,
  IngestFileResultDto,
  IntegrityVerificationDto,
  KhojRuntimeStatusDto,
  LocalModelRuntimeStatusDto,
  RetrievalRuntimeStatusDto,
  SearchHitDto,
  SearchRequestDto,
  SourceDto,
  WorkspaceDto,
} from "./contracts";

export async function listWorkspaces(): Promise<WorkspaceDto[]> {
  return invoke<WorkspaceDto[]>("workspace_list");
}

export async function createWorkspace(name: string): Promise<WorkspaceDto> {
  return invoke<WorkspaceDto>("workspace_create", { request: { name } });
}

export async function chooseSourcePaths(): Promise<string[]> {
  const selection = await open({
    multiple: true,
    directory: false,
    title: "Add files to the local knowledge library",
    filters: [
      {
        name: "Supported knowledge files",
        extensions: [
          "txt",
          "md",
          "markdown",
          "json",
          "rs",
          "ts",
          "tsx",
          "js",
          "jsx",
          "log",
          "pdf",
        ],
      },
    ],
  });
  if (selection === null) return [];
  return Array.isArray(selection) ? selection : [selection];
}

export async function ingestFiles(
  workspaceId: string,
  paths: string[],
): Promise<IngestFileResultDto[]> {
  return invoke<IngestFileResultDto[]>("source_ingest_files", {
    request: { workspaceId, paths },
  });
}

export async function listSources(workspaceId: string): Promise<SourceDto[]> {
  return invoke<SourceDto[]>("source_list", { request: { workspaceId } });
}

export async function getSource(workspaceId: string, sourceId: string): Promise<SourceDto> {
  return invoke<SourceDto>("source_get", {
    request: { workspaceId, sourceId },
  });
}

export async function executeSearch(request: SearchRequestDto): Promise<SearchHitDto[]> {
  return invoke<SearchHitDto[]>("search_execute", { request });
}

export async function resolveCitation(
  workspaceId: string,
  hit: SearchHitDto,
): Promise<CitationDto> {
  return invoke<CitationDto>("citation_resolve", {
    request: { workspaceId, hit },
  });
}

export async function listAuditEvents(workspaceId: string): Promise<AuditEventDto[]> {
  return invoke<AuditEventDto[]>("audit_list", { request: { workspaceId } });
}

export async function verifyIntegrity(
  workspaceId: string,
): Promise<IntegrityVerificationDto> {
  return invoke<IntegrityVerificationDto>("integrity_verify", {
    request: { workspaceId },
  });
}

export async function getKhojRuntimeStatus(): Promise<KhojRuntimeStatusDto> {
  return invoke<KhojRuntimeStatusDto>("khoj_runtime_status");
}

export async function startKhojRuntime(): Promise<KhojRuntimeStatusDto> {
  return invoke<KhojRuntimeStatusDto>("khoj_runtime_start");
}

export async function stopKhojRuntime(): Promise<KhojRuntimeStatusDto> {
  return invoke<KhojRuntimeStatusDto>("khoj_runtime_stop");
}

export async function restartKhojRuntime(): Promise<KhojRuntimeStatusDto> {
  return invoke<KhojRuntimeStatusDto>("khoj_runtime_restart");
}

export async function getLocalModelRuntimeStatus(): Promise<LocalModelRuntimeStatusDto> {
  return invoke<LocalModelRuntimeStatusDto>("local_model_runtime_status");
}

export async function startLocalModelRuntime(): Promise<LocalModelRuntimeStatusDto> {
  return invoke<LocalModelRuntimeStatusDto>("local_model_runtime_start");
}

export async function stopLocalModelRuntime(): Promise<LocalModelRuntimeStatusDto> {
  return invoke<LocalModelRuntimeStatusDto>("local_model_runtime_stop");
}

export async function restartLocalModelRuntime(): Promise<LocalModelRuntimeStatusDto> {
  return invoke<LocalModelRuntimeStatusDto>("local_model_runtime_restart");
}

export async function getEmbeddingRuntimeStatus(): Promise<RetrievalRuntimeStatusDto> {
  return invoke<RetrievalRuntimeStatusDto>("embedding_runtime_status");
}

export async function startEmbeddingRuntime(): Promise<RetrievalRuntimeStatusDto> {
  return invoke<RetrievalRuntimeStatusDto>("embedding_runtime_start");
}

export async function stopEmbeddingRuntime(): Promise<RetrievalRuntimeStatusDto> {
  return invoke<RetrievalRuntimeStatusDto>("embedding_runtime_stop");
}

export async function restartEmbeddingRuntime(): Promise<RetrievalRuntimeStatusDto> {
  return invoke<RetrievalRuntimeStatusDto>("embedding_runtime_restart");
}

export async function getRerankerRuntimeStatus(): Promise<RetrievalRuntimeStatusDto> {
  return invoke<RetrievalRuntimeStatusDto>("reranker_runtime_status");
}

export async function startRerankerRuntime(): Promise<RetrievalRuntimeStatusDto> {
  return invoke<RetrievalRuntimeStatusDto>("reranker_runtime_start");
}

export async function stopRerankerRuntime(): Promise<RetrievalRuntimeStatusDto> {
  return invoke<RetrievalRuntimeStatusDto>("reranker_runtime_stop");
}

export async function restartRerankerRuntime(): Promise<RetrievalRuntimeStatusDto> {
  return invoke<RetrievalRuntimeStatusDto>("reranker_runtime_restart");
}

export async function sendChat(request: ChatSendRequestDto): Promise<ChatSendResponseDto> {
  return invoke<ChatSendResponseDto>("chat_send", { request });
}

export async function listChatSessions(): Promise<ChatSessionDto[]> {
  return invoke<ChatSessionDto[]>("chat_sessions");
}

export async function getChatHistory(conversationId: string): Promise<ChatHistoryDto> {
  return invoke<ChatHistoryDto>("chat_history", {
    conversationId,
  });
}

export async function startChatStream(
  requestId: string,
  request: ChatSendRequestDto,
): Promise<ChatStreamStartResultDto> {
  return invoke<ChatStreamStartResultDto>("chat_stream_start", {
    requestId,
    request,
  });
}

export async function cancelChatStream(requestId: string): Promise<boolean> {
  return invoke<boolean>("chat_stream_cancel", { requestId });
}
