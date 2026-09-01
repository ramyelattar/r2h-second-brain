import { describe, expect, it } from "vitest";

import type { RagRetrievalDto, RagStrategy } from "./contracts";

describe("RAG retrieval DTO contract", () => {
  it("accepts the three canonical strategy values", () => {
    const strategies: RagStrategy[] = ["fts_only", "vector_only", "hybrid"];
    const retrieval = {
      strategy: "vector_only",
      ftsUsed: false,
      vectorUsed: true,
      rerankerUsed: false,
      candidateCount: 1,
      selectedCount: 1,
      evidence: [],
    } satisfies RagRetrievalDto;

    expect(strategies).toEqual(["fts_only", "vector_only", "hybrid"]);
    expect(retrieval.strategy).toBe("vector_only");
    expect(retrieval.ftsUsed).toBe(false);
  });
});
