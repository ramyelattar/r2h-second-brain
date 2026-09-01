import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

import {
  getEmbeddingRuntimeStatus,
  getRerankerRuntimeStatus,
  restartEmbeddingRuntime,
  restartRerankerRuntime,
  startEmbeddingRuntime,
  startRerankerRuntime,
  stopEmbeddingRuntime,
  stopRerankerRuntime,
} from "./client";

const embeddingStatus = {
  role: "embedding" as const,
  state: "Ready" as const,
  pid: 12001,
  endpoint: "http://127.0.0.1:42112",
  model_id: "qwen3-embedding-0.6b-r2h",
  model_path: "AI/embedding/Qwen3-Embedding-0.6B-Q8_0.gguf",
  started_at: "1785602724",
  last_error: null,
};

const rerankerStatus = {
  role: "reranker" as const,
  state: "Ready" as const,
  pid: 12002,
  endpoint: "http://127.0.0.1:42113",
  model_id: "qwen3-reranker-0.6b-r2h",
  model_path: "AI/reranker/Qwen3-Reranker-0.6B.Q4_K_M.gguf",
  started_at: "1785602732",
  last_error: null,
};

describe("retrieval runtime API contracts", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("uses the exact embedding runtime command names", async () => {
    invokeMock
      .mockResolvedValueOnce(embeddingStatus)
      .mockResolvedValueOnce({ ...embeddingStatus, state: "Starting" })
      .mockResolvedValueOnce({ ...embeddingStatus, state: "Stopping" })
      .mockResolvedValueOnce({ ...embeddingStatus, state: "Starting" });

    await expect(getEmbeddingRuntimeStatus()).resolves.toEqual(embeddingStatus);
    await startEmbeddingRuntime();
    await stopEmbeddingRuntime();
    await restartEmbeddingRuntime();

    expect(invokeMock.mock.calls).toEqual([
      ["embedding_runtime_status"],
      ["embedding_runtime_start"],
      ["embedding_runtime_stop"],
      ["embedding_runtime_restart"],
    ]);
  });

  it("uses the exact reranker runtime command names", async () => {
    invokeMock
      .mockResolvedValueOnce(rerankerStatus)
      .mockResolvedValueOnce({ ...rerankerStatus, state: "Starting" })
      .mockResolvedValueOnce({ ...rerankerStatus, state: "Stopping" })
      .mockResolvedValueOnce({ ...rerankerStatus, state: "Starting" });

    await expect(getRerankerRuntimeStatus()).resolves.toEqual(rerankerStatus);
    await startRerankerRuntime();
    await stopRerankerRuntime();
    await restartRerankerRuntime();

    expect(invokeMock.mock.calls).toEqual([
      ["reranker_runtime_status"],
      ["reranker_runtime_start"],
      ["reranker_runtime_stop"],
      ["reranker_runtime_restart"],
    ]);
  });
});
