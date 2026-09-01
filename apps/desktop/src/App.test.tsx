import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import App from "./App";
import * as client from "./api/client";

vi.mock("./api/client");

const workspaceA = {
  id: "workspace-a",
  name: "Arabic Research",
  createdAtUtc: "2026-07-19T08:00:00Z",
  archivedAtUtc: null,
};
const workspaceB = {
  id: "workspace-b",
  name: "Engineering",
  createdAtUtc: "2026-07-19T09:00:00Z",
  archivedAtUtc: null,
};

describe("R2H Second Brain desktop interface", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.mocked(client.listWorkspaces).mockResolvedValue([]);
    vi.mocked(client.listSources).mockResolvedValue([]);
    vi.mocked(client.executeSearch).mockResolvedValue([]);
    vi.mocked(client.listAuditEvents).mockResolvedValue([]);

    vi.mocked(client.getKhojRuntimeStatus).mockResolvedValue({
      state: "Ready",
      pid: 42110,
      endpoint: "http://127.0.0.1:42110",
      started_at: "2026-08-01T18:00:00Z",
      last_error: null,
    });

    vi.mocked(client.getLocalModelRuntimeStatus).mockResolvedValue({
      state: "Ready",
      pid: 42111,
      endpoint: "http://127.0.0.1:42111",
      model_id: "qwen3-4b-r2h",
      model_path: "AI/chat/qwen3-4b.gguf",
      started_at: "2026-08-01T18:00:00Z",
      last_error: null,
    });

    vi.mocked(client.verifyIntegrity).mockResolvedValue({
      workspaceId: workspaceA.id,
      eventCount: 0,
      isValid: true,
      firstFailure: null,
    });
  });

  it("creates a workspace and reports a command error without losing the form", async () => {
    const user = userEvent.setup();
    vi.mocked(client.createWorkspace)
      .mockResolvedValueOnce(workspaceA)
      .mockRejectedValueOnce({ code: "conflict", details: "Already exists" });
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Workspaces" }));
    await user.click(screen.getByRole("button", { name: /create workspace/i }));
    expect(screen.getByText(/name is required/i)).toBeVisible();
    await user.type(screen.getByLabelText(/workspace name/i), "Arabic Research");
    await user.click(screen.getByRole("button", { name: /create workspace/i }));
    expect(await screen.findByText("Arabic Research")).toBeVisible();

    await user.type(screen.getByLabelText(/workspace name/i), "Again");
    await user.click(screen.getByRole("button", { name: /create workspace/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Already exists");
    expect(screen.getByLabelText(/workspace name/i)).toHaveValue("Again");
  });

  it("shows independent success and failure results for one ingest batch", async () => {
    const user = userEvent.setup();
    vi.mocked(client.listWorkspaces).mockResolvedValue([workspaceA]);
    vi.mocked(client.chooseSourcePaths).mockResolvedValue(["allowed.md", "outside.txt"]);
    vi.mocked(client.ingestFiles).mockResolvedValue([
      {
        path: "allowed.md",
        status: "succeeded",
        sourceId: "source-1",
        documentVersionId: "version-1",
        error: null,
      },
      {
        path: "outside.txt",
        status: "failed",
        sourceId: null,
        documentVersionId: null,
        error: { code: "access_denied" },
      },
    ]);
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Imports" }));
    await user.click(screen.getByRole("button", { name: /add files/i }));
    expect(await screen.findByText("allowed.md")).toBeVisible();
    expect(screen.getByText("outside.txt")).toBeVisible();
    expect(screen.getByText(/access denied/i)).toBeVisible();
  });

  it("blocks search until a workspace and nonblank query are present", async () => {
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("button", { name: "Search" }));
    expect(screen.getByText(/select a workspace to search/i)).toBeVisible();
    expect(screen.getByRole("button", { name: /^search$/i })).toBeDisabled();

    vi.mocked(client.listWorkspaces).mockResolvedValue([workspaceA]);
    render(<App />);
    const buttons = await screen.findAllByRole("button", { name: "Search" });
    await user.click(buttons[buttons.length - 1]);
    expect(screen.getByText(/enter a search query/i)).toBeVisible();
  });

  it("resolves evidence through the citation command before showing details", async () => {
    const user = userEvent.setup();
    vi.mocked(client.listWorkspaces).mockResolvedValue([workspaceA]);
    vi.mocked(client.executeSearch).mockResolvedValue([
      {
        blockId: "block-1",
        documentVersionId: "version-1",
        sourceId: "source-1",
        score: 0.92,
        snippet: "المعرفة المحلية آمنة",
        span: { kind: "lines", startLine: 12, endLine: 14 },
      },
    ]);
    vi.mocked(client.resolveCitation).mockResolvedValue({
      sourceId: "source-1",
      displayName: "دليل.md",
      canonicalLocator: "file:///documents/دليل.md",
      contentSha256: "a".repeat(64),
      span: { kind: "lines", startLine: 12, endLine: 14 },
      excerpt: "المعرفة المحلية آمنة",
    });
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Search" }));
    await user.type(screen.getByLabelText(/search query/i), "المعرفة");
    await user.click(screen.getByRole("button", { name: /^search$/i }));
    await user.click(await screen.findByRole("button", { name: /open evidence/i }));
    await waitFor(() => expect(client.resolveCitation).toHaveBeenCalledTimes(1));
    expect(await screen.findByRole("dialog")).toHaveTextContent(
      "file:///documents/دليل.md",
    );
  });

  it("shows a clear audit verification failure banner", async () => {
    const user = userEvent.setup();
    vi.mocked(client.listWorkspaces).mockResolvedValue([workspaceA]);
    vi.mocked(client.verifyIntegrity).mockResolvedValue({
      workspaceId: workspaceA.id,
      eventCount: 57,
      isValid: false,
      firstFailure: { kind: "event_hash_mismatch", sequence: 57 },
    });
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Audit Logs" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      /chain verification failed at sequence 57/i,
    );
  });

  it("clears stale results immediately when the selected workspace changes", async () => {
    const user = userEvent.setup();
    vi.mocked(client.listWorkspaces).mockResolvedValue([workspaceA, workspaceB]);
    vi.mocked(client.executeSearch).mockResolvedValue([
      {
        blockId: "block-a",
        documentVersionId: "version-a",
        sourceId: "source-a",
        score: 0.7,
        snippet: "Workspace A result",
        span: { kind: "lines", startLine: 1, endLine: 2 },
      },
    ]);
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "Search" }));
    await user.type(screen.getByLabelText(/search query/i), "result");
    await user.click(screen.getByRole("button", { name: /^search$/i }));
    expect(await screen.findByText("Workspace A result")).toBeVisible();

    await user.selectOptions(screen.getByLabelText(/selected workspace/i), workspaceB.id);
    expect(screen.queryByText("Workspace A result")).not.toBeInTheDocument();
    expect(
      within(screen.getByRole("main")).getByText(/search your local knowledge/i),
    ).toBeVisible();
  });
});
