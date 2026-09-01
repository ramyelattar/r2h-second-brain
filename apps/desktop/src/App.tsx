import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

import * as client from "./api/client";
import type {
  AuditEventDto,
  CitationDto,
  ChatHistoryMessageDto,
  ChatKnowledgeMode,
  ChatSessionDto,
  ChatStreamEventDto,
  CommandError,
  IngestFileResultDto,
  IntegrityVerificationDto,
  KhojRuntimeStatusDto,
  LocalModelRuntimeStatusDto,
  RagEvidenceDto,
  RagRetrievalDto,
  SearchHitDto,
  SourceDto,
  SourceSpanDto,
  WorkspaceDto,
} from "./api/contracts";
import { Icon, type IconName } from "./icons";
import "./styles.css";

type Screen =
  | "dashboard"
  | "workspaces"
  | "search"
  | "citations"
  | "chat"
  | "models"
  | "imports"
  | "backup"
  | "integrity"
  | "audit"
  | "settings";

const SELECTED_WORKSPACE_KEY = "r2h.selected-workspace-id.v1";

const NAVIGATION: Array<{ id: Screen; label: string; icon: IconName }> = [
  { id: "dashboard", label: "Dashboard", icon: "dashboard" },
  { id: "workspaces", label: "Workspaces", icon: "workspace" },
  { id: "search", label: "Search", icon: "search" },
  { id: "citations", label: "Citations", icon: "citation" },
  { id: "chat", label: "AI Chat", icon: "chat" },
  { id: "models", label: "Local Models", icon: "model" },
  { id: "imports", label: "Imports", icon: "import" },
  { id: "backup", label: "Backup & Restore", icon: "backup" },
  { id: "integrity", label: "Integrity Check", icon: "integrity" },
  { id: "audit", label: "Audit Logs", icon: "log" },
  { id: "settings", label: "Settings", icon: "settings" },
];

function errorMessage(error: unknown): string {
  if (typeof error === "string" && error.trim()) {
    return error;
  }

  if (typeof error === "object" && error !== null) {
    const commandError = error as Partial<CommandError>;
    if (commandError.details) return commandError.details;
    if (commandError.code) return commandError.code.replaceAll("_", " ");
  }
  return error instanceof Error
    ? "The desktop command bridge is unavailable."
    : "Local knowledge operation failed";
}

const INITIAL_KHOJ_RUNTIME: KhojRuntimeStatusDto = {
  state: "Stopped",
  pid: null,
  endpoint: "http://127.0.0.1:42110",
  started_at: null,
  last_error: null,
};

const INITIAL_LOCAL_MODEL_RUNTIME: LocalModelRuntimeStatusDto = {
  state: "Stopped",
  pid: null,
  endpoint: "http://127.0.0.1:42111",
  model_id: "qwen3-4b-r2h",
  model_path: "",
  started_at: null,
  last_error: null,
};

function khojRuntimeLabel(status: KhojRuntimeStatusDto): string {
  switch (status.state) {
    case "Ready":
      return "Ready";
    case "Starting":
      return "Starting…";
    case "Stopping":
      return "Stopping…";
    case "Failed":
      return "Failed";
    case "Stopped":
      return "Stopped";
  }
}

function khojRuntimeTone(
  status: KhojRuntimeStatusDto,
): "success" | "danger" | "neutral" | "warning" {
  switch (status.state) {
    case "Ready":
      return "success";
    case "Failed":
      return "danger";
    case "Starting":
    case "Stopping":
      return "warning";
    case "Stopped":
      return "neutral";
  }
}

function localModelRuntimeLabel(status: LocalModelRuntimeStatusDto): string {
  switch (status.state) {
    case "Ready":
      return "Ready";
    case "Starting":
      return "Starting…";
    case "Stopping":
      return "Stopping…";
    case "Failed":
      return "Failed";
    case "Stopped":
      return "Stopped";
  }
}

function localModelRuntimeTone(
  status: LocalModelRuntimeStatusDto,
): "success" | "danger" | "neutral" | "warning" {
  switch (status.state) {
    case "Ready":
      return "success";
    case "Failed":
      return "danger";
    case "Starting":
    case "Stopping":
      return "warning";
    case "Stopped":
      return "neutral";
  }
}

function modelFileName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
}

function fileName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).at(-1) ?? path;
}

function hashPrefix(hash: string | undefined): string {
  return hash ? `${hash.slice(0, 10)}…` : "—";
}

function formatLocal(timestamp: string): string {
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(timestamp));
}

function formatUtc(timestamp: string): string {
  return new Date(timestamp).toISOString().replace(".000Z", "Z");
}

function spanLabel(span: SourceSpanDto): string {
  return span.kind === "lines"
    ? `Lines ${span.startLine}–${span.endLine}`
    : `Page ${span.page}, chars ${span.startChar}–${span.endChar}`;
}

function EmptyState({
  title,
  body,
  compact = false,
}: {
  title: string;
  body: string;
  compact?: boolean;
}) {
  return (
    <section className={compact ? "empty-state compact" : "empty-state"}>
      <span className="empty-mark" aria-hidden="true">
        ◎
      </span>
      <div>
        <h2>{title}</h2>
        <p>{body}</p>
      </div>
    </section>
  );
}

function ErrorPanel({ message }: { message: string }) {
  return (
    <div className="alert error" role="alert">
      <span aria-hidden="true">!</span>
      <p>{message}</p>
    </div>
  );
}

function StatusPill({
  tone,
  children,
}: {
  tone: "success" | "danger" | "neutral" | "warning";
  children: React.ReactNode;
}) {
  return <span className={`status-pill ${tone}`}>{children}</span>;
}

export default function App() {
  const [screen, setScreen] = useState<Screen>("dashboard");
  const [workspaces, setWorkspaces] = useState<WorkspaceDto[]>([]);
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState("");
  const [initializing, setInitializing] = useState(true);
  const [appError, setAppError] = useState("");

  const [khojRuntime, setKhojRuntime] =
    useState<KhojRuntimeStatusDto>(INITIAL_KHOJ_RUNTIME);
  const [khojRuntimeError, setKhojRuntimeError] = useState("");
  const [khojRuntimeAction, setKhojRuntimeAction] = useState(false);

  const [localModelRuntime, setLocalModelRuntime] = useState<LocalModelRuntimeStatusDto>(
    INITIAL_LOCAL_MODEL_RUNTIME,
  );
  const [localModelRuntimeError, setLocalModelRuntimeError] = useState("");
  const [localModelRuntimeAction, setLocalModelRuntimeAction] = useState(false);

  const [workspaceName, setWorkspaceName] = useState("");
  const [workspaceError, setWorkspaceError] = useState("");
  const [creatingWorkspace, setCreatingWorkspace] = useState(false);

  const [sources, setSources] = useState<SourceDto[]>([]);
  const [sourcesLoading, setSourcesLoading] = useState(false);
  const [libraryError, setLibraryError] = useState("");
  const [ingestResults, setIngestResults] = useState<IngestFileResultDto[]>([]);
  const [selectedSource, setSelectedSource] = useState<SourceDto | null>(null);
  const [ingesting, setIngesting] = useState(false);

  const [query, setQuery] = useState("");
  const [searchHits, setSearchHits] = useState<SearchHitDto[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState("");
  const [citation, setCitation] = useState<CitationDto | null>(null);
  const [citationLoadingId, setCitationLoadingId] = useState("");

  const [auditEvents, setAuditEvents] = useState<AuditEventDto[]>([]);
  const [integrity, setIntegrity] = useState<IntegrityVerificationDto | null>(null);
  const [auditLoading, setAuditLoading] = useState(false);
  const [auditError, setAuditError] = useState("");

  const selectedWorkspace = useMemo(
    () => workspaces.find((workspace) => workspace.id === selectedWorkspaceId) ?? null,
    [selectedWorkspaceId, workspaces],
  );

  useEffect(() => {
    let active = true;
    void client
      .listWorkspaces()
      .then((values) => {
        if (!active) return;
        const visible = values.filter((workspace) => !workspace.archivedAtUtc);
        setWorkspaces(visible);
        const stored = localStorage.getItem(SELECTED_WORKSPACE_KEY);
        const selection =
          visible.find((workspace) => workspace.id === stored) ?? visible[0];
        setSourcesLoading(Boolean(selection));
        setSelectedWorkspaceId(selection?.id ?? "");
      })
      .catch((error: unknown) => {
        if (active) setAppError(errorMessage(error));
      })
      .finally(() => {
        if (active) setInitializing(false);
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (selectedWorkspaceId) {
      localStorage.setItem(SELECTED_WORKSPACE_KEY, selectedWorkspaceId);
    } else {
      localStorage.removeItem(SELECTED_WORKSPACE_KEY);
      return;
    }
    let active = true;
    void client
      .listSources(selectedWorkspaceId)
      .then((values) => {
        if (active) setSources(values);
      })
      .catch((error: unknown) => {
        if (active) setLibraryError(errorMessage(error));
      })
      .finally(() => {
        if (active) setSourcesLoading(false);
      });
    return () => {
      active = false;
    };
  }, [selectedWorkspaceId]);

  useEffect(() => {
    if ((screen !== "audit" && screen !== "integrity") || !selectedWorkspaceId) return;
    let active = true;
    void Promise.all([
      client.listAuditEvents(selectedWorkspaceId),
      client.verifyIntegrity(selectedWorkspaceId),
    ])
      .then(([events, verification]) => {
        if (!active) return;
        setAuditEvents(events);
        setIntegrity(verification);
      })
      .catch((error: unknown) => {
        if (active) setAuditError(errorMessage(error));
      })
      .finally(() => {
        if (active) setAuditLoading(false);
      });
    return () => {
      active = false;
    };
  }, [screen, selectedWorkspaceId]);

  useEffect(() => {
    let active = true;

    async function refreshKhojRuntime() {
      try {
        const status = await client.getKhojRuntimeStatus();
        if (!active) return;
        setKhojRuntime(status);
        setKhojRuntimeError("");
      } catch (error) {
        if (!active) return;
        setKhojRuntimeError(errorMessage(error));
      }
    }

    void refreshKhojRuntime();
    const timer = window.setInterval(() => {
      void refreshKhojRuntime();
    }, 1_000);

    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  async function runKhojRuntimeAction(action: "start" | "stop" | "restart") {
    setKhojRuntimeAction(true);
    setKhojRuntimeError("");

    try {
      const status =
        action === "start"
          ? await client.startKhojRuntime()
          : action === "stop"
            ? await client.stopKhojRuntime()
            : await client.restartKhojRuntime();

      setKhojRuntime(status);
    } catch (error) {
      setKhojRuntimeError(errorMessage(error));
    } finally {
      setKhojRuntimeAction(false);
    }
  }

  useEffect(() => {
    let active = true;

    async function refreshLocalModelRuntime() {
      try {
        const status = await client.getLocalModelRuntimeStatus();
        if (!active) return;

        setLocalModelRuntime(status);
        setLocalModelRuntimeError("");
      } catch (error) {
        if (!active) return;
        setLocalModelRuntimeError(errorMessage(error));
      }
    }

    void refreshLocalModelRuntime();

    const timer = window.setInterval(() => {
      void refreshLocalModelRuntime();
    }, 1_000);

    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, []);

  async function runLocalModelRuntimeAction(action: "start" | "stop" | "restart") {
    setLocalModelRuntimeAction(true);
    setLocalModelRuntimeError("");

    try {
      const status =
        action === "start"
          ? await client.startLocalModelRuntime()
          : action === "stop"
            ? await client.stopLocalModelRuntime()
            : await client.restartLocalModelRuntime();

      setLocalModelRuntime(status);
    } catch (error) {
      setLocalModelRuntimeError(errorMessage(error));
    } finally {
      setLocalModelRuntimeAction(false);
    }
  }

  function changeWorkspace(nextWorkspaceId: string) {
    setSourcesLoading(Boolean(nextWorkspaceId));
    setSelectedWorkspaceId(nextWorkspaceId);
    setSearchHits([]);
    setCitation(null);
    setSelectedSource(null);
    setIngestResults([]);
    setSearchError("");
    setLibraryError("");
    setAuditEvents([]);
    setIntegrity(null);
    setAuditLoading(
      (screen === "audit" || screen === "integrity") && Boolean(nextWorkspaceId),
    );
  }

  function navigate(nextScreen: Screen) {
    setScreen(nextScreen);
    if ((nextScreen === "audit" || nextScreen === "integrity") && selectedWorkspaceId) {
      setAuditLoading(true);
      setAuditError("");
    }
  }

  async function submitWorkspace(event: React.FormEvent) {
    event.preventDefault();
    const name = workspaceName.trim();
    if (!name) {
      setWorkspaceError("Workspace name is required.");
      return;
    }
    setWorkspaceError("");
    setCreatingWorkspace(true);
    try {
      const created = await client.createWorkspace(name);
      setWorkspaces((current) => [...current, created]);
      changeWorkspace(created.id);
      setWorkspaceName("");
    } catch (error) {
      setWorkspaceError(errorMessage(error));
    } finally {
      setCreatingWorkspace(false);
    }
  }

  async function addFiles() {
    if (!selectedWorkspaceId) return;
    setLibraryError("");
    try {
      const paths = await client.chooseSourcePaths();
      if (paths.length === 0) return;
      setIngesting(true);
      const results = await client.ingestFiles(selectedWorkspaceId, paths);
      setIngestResults(results);
      setSources(await client.listSources(selectedWorkspaceId));
    } catch (error) {
      setLibraryError(errorMessage(error));
    } finally {
      setIngesting(false);
    }
  }

  async function runSearch(event: React.FormEvent) {
    event.preventDefault();
    if (!selectedWorkspaceId || !query.trim()) return;
    setSearching(true);
    setSearchError("");
    setCitation(null);
    try {
      setSearchHits(
        await client.executeSearch({
          workspaceId: selectedWorkspaceId,
          query: query.trim(),
          sourceKinds: [],
          limit: 25,
        }),
      );
    } catch (error) {
      setSearchError(errorMessage(error));
    } finally {
      setSearching(false);
    }
  }

  async function openEvidence(hit: SearchHitDto) {
    if (!selectedWorkspaceId) return;
    setCitationLoadingId(hit.blockId);
    setSearchError("");
    try {
      setCitation(await client.resolveCitation(selectedWorkspaceId, hit));
    } catch (error) {
      setSearchError(errorMessage(error));
    } finally {
      setCitationLoadingId("");
    }
  }

  function submitGlobalSearch(event: React.FormEvent) {
    navigate("search");
    void runSearch(event);
  }

  return (
    <div className="app-frame">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">R2H</span>
          <div>
            <strong>R2H Second Brain</strong>
            <span>Private knowledge system</span>
          </div>
        </div>
        <nav aria-label="Primary navigation">
          {NAVIGATION.map((item) =>
            screen === item.id ? (
              <span
                className="nav-item active"
                key={item.id}
                aria-current="page"
                aria-label={item.label}
              >
                <Icon name={item.icon} />
                <span>{item.label}</span>
              </span>
            ) : (
              <button
                className="nav-item"
                key={item.id}
                type="button"
                aria-label={item.label}
                onClick={() => navigate(item.id)}
              >
                <Icon name={item.icon} />
                <span>{item.label}</span>
              </button>
            ),
          )}
        </nav>
        <div className="local-note">
          <span className="local-dot" />
          <div>
            <strong>Local mode</strong>
            <p>All knowledge stays on this device.</p>
          </div>
        </div>
        <div className="profile-card">
          <span className="profile-mark">R2</span>
          <div>
            <strong>Private session</strong>
            <span>Local account</span>
          </div>
          <Icon name="chevron" />
        </div>
      </aside>

      <div className="workspace">
        <header className="topbar">
          <form className="global-search" onSubmit={submitGlobalSearch}>
            <Icon name="search" />
            <input
              aria-label="Global search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search everything…"
            />
            <kbd>⌘ K</kbd>
          </form>
          <label className="workspace-picker">
            <span className="local-dot" />
            <select
              aria-label="Selected workspace"
              value={selectedWorkspaceId}
              onChange={(event) => changeWorkspace(event.target.value)}
              disabled={workspaces.length === 0}
            >
              {workspaces.length === 0 ? (
                <option value="">Local mode · No workspace</option>
              ) : (
                workspaces.map((workspace) => (
                  <option key={workspace.id} value={workspace.id}>
                    Local · {workspace.name}
                  </option>
                ))
              )}
            </select>
          </label>
        </header>

        <main>
          {appError && <ErrorPanel message={appError} />}
          {initializing && (
            <p className="loading-banner" role="status">
              Opening local knowledge…
            </p>
          )}
          <div className="screen-stage" key={screen}>
            {screen === "dashboard" && (
              <DashboardScreen
                workspaces={workspaces}
                selectedWorkspace={selectedWorkspace}
                sources={sources}
                integrity={integrity}
                khojRuntime={khojRuntime}
                localModelRuntime={localModelRuntime}
                onNavigate={navigate}
                onAddFiles={addFiles}
              />
            )}
            {screen === "workspaces" && (
              <WorkspaceScreen
                workspaces={workspaces}
                selectedWorkspaceId={selectedWorkspaceId}
                workspaceName={workspaceName}
                workspaceError={workspaceError}
                creating={creatingWorkspace}
                onNameChange={setWorkspaceName}
                onSubmit={submitWorkspace}
                onSelect={changeWorkspace}
              />
            )}
            {screen === "imports" && (
              <LibraryScreen
                workspace={selectedWorkspace}
                sources={sources}
                loading={sourcesLoading}
                error={libraryError}
                ingesting={ingesting}
                ingestResults={ingestResults}
                selectedSource={selectedSource}
                onAddFiles={addFiles}
                onSelectSource={setSelectedSource}
                onCloseSource={() => setSelectedSource(null)}
              />
            )}
            {screen === "search" && (
              <SearchScreen
                workspace={selectedWorkspace}
                sources={sources}
                query={query}
                hits={searchHits}
                searching={searching}
                error={searchError}
                citation={citation}
                citationLoadingId={citationLoadingId}
                onQueryChange={setQuery}
                onSubmit={runSearch}
                onOpenEvidence={openEvidence}
                onCloseCitation={() => setCitation(null)}
              />
            )}
            {screen === "citations" && (
              <CitationsScreen
                workspace={selectedWorkspace}
                sources={sources}
                selectedSource={selectedSource}
                onSelectSource={setSelectedSource}
                onCloseSource={() => setSelectedSource(null)}
              />
            )}
            {screen === "chat" && (
              <ChatScreen
                workspace={selectedWorkspace}
                sources={sources}
                khojRuntime={khojRuntime}
                localModelRuntime={localModelRuntime}
              />
            )}
            {screen === "models" && (
              <ModelsScreen
                khojRuntime={khojRuntime}
                khojError={khojRuntimeError}
                khojActionPending={khojRuntimeAction}
                onKhojAction={runKhojRuntimeAction}
                localModelRuntime={localModelRuntime}
                localModelError={localModelRuntimeError}
                localModelActionPending={localModelRuntimeAction}
                onLocalModelAction={runLocalModelRuntimeAction}
              />
            )}
            {screen === "backup" && <BackupScreen workspace={selectedWorkspace} />}
            {screen === "integrity" && (
              <IntegrityScreen
                workspace={selectedWorkspace}
                integrity={integrity}
                loading={auditLoading}
                error={auditError}
                eventCount={auditEvents.length}
              />
            )}
            {screen === "audit" && (
              <AuditScreen
                workspace={selectedWorkspace}
                events={auditEvents}
                integrity={integrity}
                loading={auditLoading}
                error={auditError}
              />
            )}
            {screen === "settings" && (
              <SettingsScreen
                khojRuntime={khojRuntime}
                localModelRuntime={localModelRuntime}
              />
            )}
          </div>
        </main>
      </div>
    </div>
  );
}

function PageHeader({
  title,
  description,
  action,
}: {
  title: string;
  description: string;
  action?: React.ReactNode;
}) {
  return (
    <header className="page-header">
      <div>
        <h1>{title}</h1>
        <p>{description}</p>
      </div>
      {action}
    </header>
  );
}

function PanelHeading({
  title,
  meta,
  action,
}: {
  title: string;
  meta?: string;
  action?: React.ReactNode;
}) {
  return (
    <div className="panel-heading">
      <div>
        <h2>{title}</h2>
        {meta && <p>{meta}</p>}
      </div>
      {action}
    </div>
  );
}

function DashboardScreen({
  workspaces,
  selectedWorkspace,
  sources,
  integrity,
  khojRuntime,
  localModelRuntime,
  onNavigate,
  onAddFiles,
}: {
  workspaces: WorkspaceDto[];
  selectedWorkspace: WorkspaceDto | null;
  sources: SourceDto[];
  integrity: IntegrityVerificationDto | null;
  khojRuntime: KhojRuntimeStatusDto;
  localModelRuntime: LocalModelRuntimeStatusDto;
  onNavigate: (screen: Screen) => void;
  onAddFiles: () => void;
}) {
  const versions = sources.reduce((total, source) => total + source.versions.length, 0);
  const recentSources = [...sources]
    .sort((left, right) => right.lastSeenAtUtc.localeCompare(left.lastSeenAtUtc))
    .slice(0, 4);

  return (
    <div className="dashboard-layout">
      <section className="dashboard-main">
        <section className="dashboard-hero">
          <div className="hero-copy">
            <h1>
              Your knowledge.
              <br />
              Entirely yours.
            </h1>
            <p>A private, evidence-first intelligence layer for everything you know.</p>
            <div className="hero-metrics">
              <MetricCell icon="workspace" value={workspaces.length} label="Workspaces" />
              <MetricCell icon="file" value={sources.length} label="Sources" />
              <MetricCell icon="archive" value={versions} label="Versions" />
              <MetricCell
                icon="citation"
                value={integrity?.eventCount ?? "—"}
                label="Verified events"
              />
            </div>
          </div>
          <div className="knowledge-orb" aria-hidden="true">
            <span className="orb-core" />
            <span className="orb-ring ring-one" />
            <span className="orb-ring ring-two" />
            <span className="orb-base" />
          </div>
        </section>

        <div className="dashboard-columns">
          <section className="glass-panel activity-panel">
            <PanelHeading
              title="Recent activity"
              meta={selectedWorkspace ? selectedWorkspace.name : "No workspace selected"}
              action={
                <button className="text-button" onClick={() => onNavigate("imports")}>
                  View all
                </button>
              }
            />
            {recentSources.length === 0 ? (
              <EmptyState
                compact
                title="No activity yet"
                body="Imported sources will appear here with their real local timestamps."
              />
            ) : (
              <div className="activity-list">
                {recentSources.map((source) => (
                  <button
                    key={source.id}
                    type="button"
                    onClick={() => onNavigate("imports")}
                  >
                    <span className="item-icon">
                      <Icon name="file" />
                    </span>
                    <span>
                      <strong>{source.displayName}</strong>
                      <small>{source.kind.replaceAll("_", " ")}</small>
                    </span>
                    <time>{formatLocal(source.lastSeenAtUtc)}</time>
                    <Icon name="chevron" />
                  </button>
                ))}
              </div>
            )}
          </section>

          <div className="dashboard-actions">
            <section className="glass-panel quick-panel">
              <PanelHeading title="Quick actions" />
              <div className="quick-grid">
                <QuickAction
                  icon="plus"
                  label="New workspace"
                  onClick={() => onNavigate("workspaces")}
                />
                <QuickAction
                  icon="import"
                  label="Import files"
                  onClick={onAddFiles}
                  disabled={!selectedWorkspace}
                />
                <QuickAction
                  icon="search"
                  label="Search knowledge"
                  onClick={() => onNavigate("search")}
                />
                <QuickAction
                  icon="chat"
                  label="AI research"
                  onClick={() => onNavigate("chat")}
                />
              </div>
            </section>
            <section className="glass-panel assistant-panel">
              <PanelHeading
                title="AI assistant"
                meta={`R2H Core · ${khojRuntimeLabel(khojRuntime)} · Qwen3 · ${localModelRuntimeLabel(localModelRuntime)}`}
              />
              <button
                type="button"
                className="assistant-prompt"
                onClick={() => onNavigate("chat")}
              >
                <span>Ask your second brain</span>
                <Icon name="send" />
              </button>
            </section>
          </div>
        </div>

        <section className="health-strip">
          <HealthCard
            icon="integrity"
            label="Integrity"
            value={
              integrity
                ? integrity.isValid
                  ? "Verified"
                  : "Attention required"
                : "Unverified"
            }
            detail={
              integrity
                ? `${integrity.eventCount} audit events checked`
                : "Run an integrity check"
            }
            onClick={() => onNavigate("integrity")}
          />
          <HealthCard
            icon="database"
            label="Storage"
            value={`${sources.length} local sources`}
            detail="No external storage connection"
            onClick={() => onNavigate("imports")}
          />
          <HealthCard
            icon="backup"
            label="Backup"
            value="Not configured"
            detail="No backup command connected"
            onClick={() => onNavigate("backup")}
          />
        </section>
      </section>

      <aside className="dashboard-rail">
        <section className="glass-panel overview-panel">
          <PanelHeading title="Knowledge overview" />
          <div className="overview-ring">
            <strong>{sources.length}</strong>
            <span>local sources</span>
          </div>
          <div className="overview-rows">
            <SignalRow label="Workspaces" value={workspaces.length} />
            <SignalRow label="Sources" value={sources.length} />
            <SignalRow label="Versions" value={versions} />
          </div>
        </section>
        <section className="glass-panel workspace-summary">
          <PanelHeading
            title="Workspaces"
            action={
              <button className="text-button" onClick={() => onNavigate("workspaces")}>
                Manage
              </button>
            }
          />
          {workspaces.slice(0, 4).map((workspace) => (
            <button
              key={workspace.id}
              type="button"
              onClick={() => onNavigate("workspaces")}
              className={workspace.id === selectedWorkspace?.id ? "selected" : ""}
            >
              <span className="item-icon">
                <Icon name="folder" />
              </span>
              <span>
                <strong>{workspace.name}</strong>
                <small>
                  {workspace.id === selectedWorkspace?.id ? "Selected" : "Local"}
                </small>
              </span>
              <Icon name="chevron" />
            </button>
          ))}
          {workspaces.length === 0 && <p className="quiet-copy">No workspaces yet.</p>}
        </section>
        <section className="glass-panel system-panel">
          <PanelHeading title="System" />
          <SignalRow label="Mode" value="Local only" />
          <SignalRow
            label="R2H Intelligence Core"
            value={
              <StatusPill tone={khojRuntimeTone(khojRuntime)}>
                {khojRuntimeLabel(khojRuntime)}
              </StatusPill>
            }
          />
          <SignalRow label="Cloud services" value="None" />
          <SignalRow label="Workspace" value={selectedWorkspace?.name ?? "None"} />
        </section>
      </aside>
    </div>
  );
}

function MetricCell({
  icon,
  value,
  label,
}: {
  icon: IconName;
  value: React.ReactNode;
  label: string;
}) {
  return (
    <div className="metric-cell">
      <span>
        <Icon name={icon} />
      </span>
      <strong>{value}</strong>
      <small>{label}</small>
    </div>
  );
}

function QuickAction({
  icon,
  label,
  onClick,
  disabled = false,
}: {
  icon: IconName;
  label: string;
  onClick: () => void;
  disabled?: boolean;
}) {
  return (
    <button type="button" className="quick-action" onClick={onClick} disabled={disabled}>
      <Icon name={icon} />
      <span>{label}</span>
      <Icon name="chevron" />
    </button>
  );
}

function HealthCard({
  icon,
  label,
  value,
  detail,
  onClick,
}: {
  icon: IconName;
  label: string;
  value: string;
  detail: string;
  onClick: () => void;
}) {
  return (
    <button type="button" className="glass-panel health-card" onClick={onClick}>
      <span className="health-icon">
        <Icon name={icon} />
      </span>
      <span>
        <small>{label}</small>
        <strong>{value}</strong>
        <em>{detail}</em>
      </span>
      <Icon name="chevron" />
    </button>
  );
}

function SignalRow({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="signal-row">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function WorkspaceScreen({
  workspaces,
  selectedWorkspaceId,
  workspaceName,
  workspaceError,
  creating,
  onNameChange,
  onSubmit,
  onSelect,
}: {
  workspaces: WorkspaceDto[];
  selectedWorkspaceId: string;
  workspaceName: string;
  workspaceError: string;
  creating: boolean;
  onNameChange: (value: string) => void;
  onSubmit: (event: React.FormEvent) => void;
  onSelect: (id: string) => void;
}) {
  return (
    <div className="stack">
      <PageHeader
        title="Workspaces"
        description="Private partitions for focused knowledge, evidence, and local activity."
      />
      <div className="screen-grid workspaces-grid">
        <section>
          <div className="section-heading">
            <div>
              <h2>Your workspaces</h2>
              <p>Choose the local knowledge boundary you want to work inside.</p>
            </div>
            <span className="count">{workspaces.length}</span>
          </div>
          {workspaces.length === 0 ? (
            <EmptyState
              title="Your knowledge begins here"
              body="Create a workspace to organize files, search indexed evidence, and inspect its audit history. Everything remains local to this device."
            />
          ) : (
            <div className="workspace-cards">
              {workspaces.map((workspace) => (
                <button
                  type="button"
                  className={
                    workspace.id === selectedWorkspaceId
                      ? "workspace-card selected"
                      : "workspace-card"
                  }
                  key={workspace.id}
                  onClick={() => onSelect(workspace.id)}
                >
                  <span className="workspace-monogram">
                    {workspace.name.slice(0, 2).toUpperCase()}
                  </span>
                  <span>
                    <strong>{workspace.name}</strong>
                    <small>Created {formatLocal(workspace.createdAtUtc)}</small>
                  </span>
                  <span className="card-arrow" aria-hidden="true">
                    →
                  </span>
                </button>
              ))}
            </div>
          )}
        </section>

        <aside className="form-card">
          <span className="form-icon">
            <Icon name="workspace" />
          </span>
          <h2>Create a focused space</h2>
          <p>
            Keep sources, citations, search results, and audit events inside one isolated
            local boundary.
          </p>
          <form onSubmit={onSubmit}>
            <label>
              <span>Workspace name</span>
              <input
                aria-label="Workspace name"
                value={workspaceName}
                onChange={(event) => onNameChange(event.target.value)}
                placeholder="e.g. Product research"
                maxLength={120}
              />
            </label>
            {workspaceError && <ErrorPanel message={workspaceError} />}
            <button className="primary-button" type="submit" disabled={creating}>
              <Icon name="plus" />
              {creating ? "Creating…" : "Create workspace"}
            </button>
          </form>
          <div className="privacy-rule">
            <strong>No account. No sync.</strong>
            <span>Only the selected workspace ID is saved in UI preferences.</span>
          </div>
        </aside>
      </div>
    </div>
  );
}

function LibraryScreen({
  workspace,
  sources,
  loading,
  error,
  ingesting,
  ingestResults,
  selectedSource,
  onAddFiles,
  onSelectSource,
  onCloseSource,
}: {
  workspace: WorkspaceDto | null;
  sources: SourceDto[];
  loading: boolean;
  error: string;
  ingesting: boolean;
  ingestResults: IngestFileResultDto[];
  selectedSource: SourceDto | null;
  onAddFiles: () => void;
  onSelectSource: (source: SourceDto) => void;
  onCloseSource: () => void;
}) {
  if (!workspace) {
    return (
      <div className="stack">
        <PageHeader
          title="Imports"
          description="Bring files into an isolated workspace as immutable local sources."
        />
        <EmptyState
          title="Select a workspace to open its library"
          body="Sources are always scoped to one local workspace."
        />
      </div>
    );
  }
  return (
    <div className="stack">
      <PageHeader
        title="Imports"
        description={`Bring files into ${workspace.name} as immutable, locally indexed sources.`}
        action={
          <button
            className="primary-button"
            type="button"
            onClick={onAddFiles}
            disabled={ingesting}
          >
            <Icon name="plus" />
            {ingesting ? "Adding files…" : "Add files"}
          </button>
        }
      />
      <button
        className="import-dropzone"
        type="button"
        onClick={onAddFiles}
        disabled={ingesting}
      >
        <span>
          <Icon name="import" />
        </span>
        <strong>
          {ingesting ? "Processing selected files…" : "Choose files to import"}
        </strong>
        <small>
          Files remain on this device and are indexed inside the selected workspace.
        </small>
      </button>
      {error && <ErrorPanel message={error} />}
      {ingestResults.length > 0 && (
        <section className="result-strip" aria-label="Ingestion results">
          {ingestResults.map((result, index) => (
            <article key={`${result.path}-${index}`}>
              <StatusPill tone={result.status === "succeeded" ? "success" : "danger"}>
                {result.status}
              </StatusPill>
              <strong>{fileName(result.path)}</strong>
              {fileName(result.path) !== result.path && (
                <span className="mono">{result.path}</span>
              )}
              {result.error && <small>{errorMessage(result.error)}</small>}
            </article>
          ))}
        </section>
      )}
      <section className="data-card">
        <div className="section-heading padded">
          <div>
            <h2>Imported sources</h2>
            <p>Immutable versions, parser state, and source identity.</p>
          </div>
          <span className="count">{sources.length}</span>
        </div>
        {loading ? (
          <p className="loading-copy">Reading source records…</p>
        ) : sources.length === 0 ? (
          <EmptyState
            compact
            title="No sources yet"
            body="Choose supported files to create local, content-addressed versions."
          />
        ) : (
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>Source</th>
                  <th>Type</th>
                  <th>Latest status</th>
                  <th>Version hash</th>
                  <th>Last seen</th>
                  <th aria-label="Actions" />
                </tr>
              </thead>
              <tbody>
                {sources.map((source) => {
                  const latest = source.versions[0];
                  return (
                    <tr key={source.id}>
                      <td>
                        <strong>{source.displayName}</strong>
                        <small className="mono">{source.id.slice(0, 8)}</small>
                      </td>
                      <td>{source.kind.replaceAll("_", " ")}</td>
                      <td>
                        <StatusPill
                          tone={
                            latest?.status === "ready"
                              ? "success"
                              : latest?.status === "failed"
                                ? "danger"
                                : "warning"
                          }
                        >
                          {latest?.status ?? "unknown"}
                        </StatusPill>
                      </td>
                      <td className="mono">{hashPrefix(latest?.contentSha256)}</td>
                      <td>{formatLocal(source.lastSeenAtUtc)}</td>
                      <td>
                        <button
                          className="text-button"
                          type="button"
                          onClick={() => onSelectSource(source)}
                        >
                          View details
                        </button>
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </section>
      {selectedSource && <SourceDetails source={selectedSource} onClose={onCloseSource} />}
    </div>
  );
}

function SourceDetails({ source, onClose }: { source: SourceDto; onClose: () => void }) {
  return (
    <section className="drawer-panel" aria-label="Source details">
      <div className="drawer-heading">
        <div>
          <span className="kicker">Source details</span>
          <h2>{source.displayName}</h2>
        </div>
        <button type="button" className="quiet-button" onClick={onClose}>
          Close
        </button>
      </div>
      <dl className="source-metadata">
        <div>
          <dt>Type</dt>
          <dd>{source.kind.replaceAll("_", " ")}</dd>
        </div>
        <div>
          <dt>Last seen</dt>
          <dd>{formatLocal(source.lastSeenAtUtc)}</dd>
        </div>
        <div className="wide">
          <dt>Canonical locator</dt>
          <dd className="mono">{source.canonicalLocator}</dd>
        </div>
      </dl>
      <h3>Immutable versions</h3>
      <div className="version-list">
        {source.versions.map((version) => (
          <article key={version.id}>
            <div>
              <StatusPill
                tone={
                  version.status === "ready"
                    ? "success"
                    : version.status === "failed"
                      ? "danger"
                      : "warning"
                }
              >
                {version.status}
              </StatusPill>
              <strong className="mono">{hashPrefix(version.contentSha256)}</strong>
            </div>
            <p>
              {version.parserId} · {version.parserVersion} ·{" "}
              {formatLocal(version.createdAtUtc)}
            </p>
            {version.status === "failed" && (
              <ErrorPanel
                message={`${version.failureCode ?? "parser_failure"}: ${
                  version.failureMessage ?? "Parsing failed"
                }`}
              />
            )}
          </article>
        ))}
      </div>
    </section>
  );
}

function CitationsScreen({
  workspace,
  sources,
  selectedSource,
  onSelectSource,
  onCloseSource,
}: {
  workspace: WorkspaceDto | null;
  sources: SourceDto[];
  selectedSource: SourceDto | null;
  onSelectSource: (source: SourceDto) => void;
  onCloseSource: () => void;
}) {
  return (
    <div className="stack">
      <PageHeader
        title="Citations"
        description="Inspect source identity, immutable versions, and canonical evidence paths."
      />
      {!workspace ? (
        <EmptyState
          title="Select a workspace to browse citations"
          body="Citation records are isolated to a single local workspace."
        />
      ) : (
        <div className="citation-browser">
          <section className="glass-panel citation-list">
            <PanelHeading
              title="Source traceability"
              meta={`${sources.length} local sources`}
            />
            <div className="filter-bar">
              <label className="filter-search">
                <Icon name="search" />
                <input aria-label="Filter citations" placeholder="Filter sources…" />
              </label>
              <button type="button" className="quiet-button">
                All source types
              </button>
            </div>
            {sources.length === 0 ? (
              <EmptyState
                compact
                title="No citation records"
                body="Import sources and run a search to resolve traceable evidence."
              />
            ) : (
              <div className="citation-source-list">
                {sources.map((source) => {
                  const latest = source.versions[0];
                  return (
                    <button
                      key={source.id}
                      type="button"
                      className={selectedSource?.id === source.id ? "selected" : ""}
                      onClick={() => onSelectSource(source)}
                    >
                      <span className="item-icon">
                        <Icon name="file" />
                      </span>
                      <span>
                        <strong>{source.displayName}</strong>
                        <small>{source.canonicalLocator}</small>
                      </span>
                      <span className="source-proof">
                        <strong>{hashPrefix(latest?.contentSha256)}</strong>
                        <small>{latest?.status ?? "unknown"}</small>
                      </span>
                      <Icon name="chevron" />
                    </button>
                  );
                })}
              </div>
            )}
          </section>
          <aside className="glass-panel evidence-preview">
            {selectedSource ? (
              <SourceDetails source={selectedSource} onClose={onCloseSource} />
            ) : (
              <>
                <span className="evidence-mark">
                  <Icon name="citation" />
                </span>
                <h2>Evidence preview</h2>
                <p>
                  Select a source to inspect its canonical locator, parser history, and
                  immutable content hashes.
                </p>
                <div className="trust-lines">
                  <SignalRow label="Workspace" value={workspace.name} />
                  <SignalRow label="Storage" value="Local" />
                  <SignalRow label="Resolution" value="On demand" />
                </div>
              </>
            )}
          </aside>
        </div>
      )}
    </div>
  );
}

function RagMessageText({
  text,
  evidence,
  onOpenEvidence,
}: {
  text: string;
  evidence: RagEvidenceDto[];
  onOpenEvidence: (evidence: RagEvidenceDto) => void;
}) {
  const validNumbers = new Set(evidence.map((item) => item.citationNumber));

  const parts = text.split(/(\[\d+\])/g);

  return (
    <>
      {parts.map((part, index) => {
        const match = /^\[(\d+)\]$/.exec(part);

        if (!match) {
          return <span key={`text-${index}`}>{part}</span>;
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
          (candidate) => candidate.citationNumber === citationNumber,
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
          <span className={retrieval.ftsUsed ? "active" : ""}>FTS</span>
          <span className={retrieval.vectorUsed ? "active" : ""}>Vector</span>
          <span className={retrieval.rerankerUsed ? "active" : ""}>Reranker</span>
        </div>
      </div>

      <div className="rag-source-list">
        {retrieval.evidence.map((item) => (
          <article
            className="rag-source-card"
            key={`${item.blockId}-${item.citationNumber}`}
          >
            <div className="rag-source-number">[{item.citationNumber}]</div>

            <div className="rag-source-body">
              <div className="rag-source-meta">
                <strong>{item.displayName}</strong>
                <span>{spanLabel(item.span)}</span>
                <span>{Math.round(item.score * 100)}% relevance</span>
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

                <code title={item.contentSha256}>{hashPrefix(item.contentSha256)}</code>
              </div>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}

function ChatScreen({
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
  const [chatMode, setChatMode] = useState<ChatKnowledgeMode>(() => {
    const stored = localStorage.getItem("r2h.chat-mode.v1");

    return stored === "ask_workspace" || stored === "evidence_only" ? stored : "general";
  });
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [sessions, setSessions] = useState<ChatSessionDto[]>([]);
  const [messages, setMessages] = useState<ChatHistoryMessageDto[]>([]);
  const [activeRequestId, setActiveRequestId] = useState<string | null>(null);
  const [streamStatus, setStreamStatus] = useState("");
  const [streamListenerReady, setStreamListenerReady] = useState(false);
  const [loadingHistory, setLoadingHistory] = useState(false);
  const [error, setError] = useState("");
  const [selectedSourceIds, setSelectedSourceIds] = useState<string[]>([]);
  const [ragByTurn, setRagByTurn] = useState<Record<string, RagRetrievalDto>>({});
  const [previewEvidence, setPreviewEvidence] = useState<RagEvidenceDto | null>(null);

  const threadRef = useRef<HTMLDivElement | null>(null);
  const activeRequestRef = useRef<string | null>(null);
  const assistantTurnRef = useRef<string | null>(null);
  const conversationIdRef = useRef<string | null>(null);
  const pendingRagRef = useRef<RagRetrievalDto | null>(null);

  const runtimeReady = khojRuntime.state === "Ready" && localModelRuntime.state === "Ready";

  const sending = activeRequestId !== null;
  const chatReady = runtimeReady && streamListenerReady;
  const visibleSessions = khojRuntime.state === "Ready" ? sessions : [];

  useEffect(() => {
    conversationIdRef.current = conversationId;
  }, [conversationId]);

  useEffect(() => {
    localStorage.setItem("r2h.chat-mode.v1", chatMode);
  }, [chatMode]);

  const availableSourceIds = new Set(sources.map((source) => source.id));

  const scopedSelectedSourceIds =
    chatMode === "general"
      ? []
      : selectedSourceIds.filter((sourceId) => availableSourceIds.has(sourceId));

  const refreshSessions = useCallback(async (): Promise<ChatSessionDto[]> => {
    if (khojRuntime.state !== "Ready") return [];

    const values = await client.listChatSessions();
    setSessions(values);

    return values;
  }, [khojRuntime.state]);

  const reconcileConversation = useCallback(
    async (id?: string | null) => {
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
      setRagByTurn(history.response.rag_by_turn ?? {});
    },
    [refreshSessions],
  );

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

        case "status":
          setStreamStatus(
            typeof payload.data === "string"
              ? payload.data.replaceAll("*", "")
              : "Generating locally…",
          );
          break;

        case "response-start": {
          const turnId = assistantTurnRef.current ?? crypto.randomUUID();

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

          if (pendingRagRef.current) {
            const retrieval = pendingRagRef.current;

            setRagByTurn((current) => ({
              ...current,
              [turnId]: retrieval,
            }));
          }

          break;
        }

        case "token": {
          const token = typeof payload.data === "string" ? payload.data : "";

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
                if (message.by === "you" || message.turnId !== finishedTurnId) {
                  return [message];
                }

                if (!message.message.trim()) {
                  return [];
                }

                return [
                  {
                    ...message,
                    message: `${message.message}

Generation stopped.`,
                  },
                ];
              }),
            );
          }

          if (!data.localOnly) {
            void reconcileConversation(data.conversationId).catch((reason: unknown) => {
              setError(errorMessage(reason));
            });
          }

          break;
        }

        case "error":
          activeRequestRef.current = null;
          assistantTurnRef.current = null;

          setActiveRequestId(null);
          setStreamStatus("");

          setError(
            typeof payload.data === "string" ? payload.data : "Local generation failed.",
          );

          break;
      }
    })
      .then((dispose) => {
        if (disposed) {
          dispose();
          return;
        }

        unlisten = dispose;
        setStreamListenerReady(true);
      })
      .catch((reason: unknown) => {
        if (!disposed) {
          setStreamListenerReady(false);
          setError(`Unable to initialize local chat events: ${errorMessage(reason)}`);
        }
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [reconcileConversation]);

  useEffect(() => {
    if (khojRuntime.state !== "Ready") {
      return undefined;
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

      if (
        history.response.workspace_id &&
        workspace &&
        history.response.workspace_id !== workspace.id
      ) {
        throw new Error(
          "This conversation belongs to a different workspace. " +
            "Select its workspace or start a new conversation.",
        );
      }

      conversationIdRef.current = history.response.conversation_id;
      setConversationId(history.response.conversation_id);
      setMessages(history.response.chat);
      setRagByTurn(history.response.rag_by_turn ?? {});
      setPreviewEvidence(null);
      pendingRagRef.current = null;
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
    setSelectedSourceIds([]);
    setRagByTurn({});
    setPreviewEvidence(null);
    pendingRagRef.current = null;
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();

    const value = draft.trim();

    if (!value || !chatReady || sending) return;

    if (chatMode !== "general" && !workspace) {
      setError("Select a workspace before using grounded chat.");
      return;
    }

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
        query: chatMode === "general" ? `/general ${value}` : value,
        conversation_id: conversationIdRef.current,
        create_new: !conversationIdRef.current,
        mode: chatMode,
        workspace_id: chatMode === "general" ? null : (workspace?.id ?? null),
        selected_source_ids: scopedSelectedSourceIds,
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
                  ? (visibleSessions.find(
                      (session) => session.conversation_id === conversationId,
                    )?.slug ?? "Private conversation")
                  : "New private conversation"}
              </h2>

              <p>{workspace?.name ?? "Local knowledge environment"}</p>
            </div>

            <StatusPill tone={chatReady ? "success" : "warning"}>
              {!runtimeReady
                ? "Runtime unavailable"
                : streamListenerReady
                  ? "R2H AI ready"
                  : "Preparing chat…"}
            </StatusPill>
          </div>

          <div className="thread-body live-chat-thread" ref={threadRef}>
            {loadingHistory ? (
              <p className="loading-copy">Loading conversation…</p>
            ) : messages.length === 0 ? (
              <article className="assistant-message">
                <div className="message-role">
                  <Icon name="spark" /> R2H Second Brain
                </div>

                <h2>Ask your private second brain.</h2>

                <p>
                  Conversations run through the local R2H Intelligence Core and remain
                  stored on this device.
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

                    <p className="assistant-answer-text">
                      {message.message ? (
                        <RagMessageText
                          text={message.message}
                          evidence={ragByTurn[message.turnId]?.evidence ?? []}
                          onOpenEvidence={setPreviewEvidence}
                        />
                      ) : (
                        <span className="stream-caret">Generating…</span>
                      )}
                    </p>

                    {ragByTurn[message.turnId] && (
                      <RagSourceCards
                        retrieval={ragByTurn[message.turnId]}
                        onOpenEvidence={setPreviewEvidence}
                      />
                    )}
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

        <div className="chat-mode-bar">
          <label>
            <span>Knowledge mode</span>
            <select
              aria-label="Chat knowledge mode"
              value={chatMode}
              onChange={(event) => {
                const nextMode = event.target.value as ChatKnowledgeMode;

                if (nextMode === "general") {
                  setSelectedSourceIds([]);
                }

                setChatMode(nextMode);
              }}
              disabled={sending}
            >
              <option value="general">General</option>
              <option value="ask_workspace" disabled={!workspace}>
                Ask Workspace
              </option>
              <option value="evidence_only" disabled={!workspace}>
                Evidence Only
              </option>
            </select>
          </label>

          <span className="chat-grounding-summary">
            {chatMode === "general"
              ? "No workspace content will be used"
              : `${workspace?.name ?? "No workspace"} · FTS · ${
                  scopedSelectedSourceIds.length === 0
                    ? "All sources"
                    : `${scopedSelectedSourceIds.length} selected`
                }`}
          </span>
        </div>

        {chatMode !== "general" && (
          <details className="chat-source-scope">
            <summary>
              <span>Source scope</span>
              <strong>
                {scopedSelectedSourceIds.length === 0
                  ? "All workspace sources"
                  : `${scopedSelectedSourceIds.length} selected`}
              </strong>
            </summary>

            <div className="chat-source-options">
              <button
                type="button"
                className="text-button"
                onClick={() => setSelectedSourceIds([])}
                disabled={scopedSelectedSourceIds.length === 0 || sending}
              >
                Use all sources
              </button>

              {sources.length === 0 ? (
                <p>No indexed sources are available.</p>
              ) : (
                sources.map((source) => (
                  <label className="chat-source-option" key={source.id}>
                    <input
                      type="checkbox"
                      checked={scopedSelectedSourceIds.includes(source.id)}
                      disabled={sending}
                      onChange={(event) => {
                        setSelectedSourceIds(() =>
                          event.target.checked
                            ? [...scopedSelectedSourceIds, source.id]
                            : scopedSelectedSourceIds.filter(
                                (sourceId) => sourceId !== source.id,
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

        <form className="chat-composer" onSubmit={submit}>
          <button type="button" aria-label="Attach context" disabled>
            <Icon name="plus" />
          </button>

          <input
            aria-label="Ask your second brain"
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            placeholder={
              !runtimeReady
                ? "Waiting for local AI runtime…"
                : !streamListenerReady
                  ? "Preparing secure local chat…"
                  : "Ask your second brain"
            }
            disabled={!chatReady || sending}
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
              disabled={!draft.trim() || !chatReady}
              aria-label="Send prompt"
            >
              <Icon name="send" />
              <span className="sr-only">Send prompt</span>
            </button>
          )}
        </form>
      </section>

      {previewEvidence && (
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
                <span>Citation [{previewEvidence.citationNumber}]</span>
                <h2 id="evidence-preview-title">{previewEvidence.displayName}</h2>
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

            <pre className="evidence-preview-content">{previewEvidence.excerpt}</pre>

            <div className="evidence-preview-provenance">
              <span>Immutable source hash</span>
              <code>{previewEvidence.contentSha256}</code>
              <span>Stored locator</span>
              <code>{previewEvidence.canonicalLocator}</code>
            </div>
          </section>
        </div>
      )}

      <aside className="glass-panel context-drawer">
        <PanelHeading
          title="Conversations"
          meta={`${visibleSessions.length} recent local sessions`}
        />

        <div className="chat-session-list">
          {visibleSessions.length === 0 ? (
            <div className="context-state">
              <span>
                <Icon name="chat" />
              </span>

              <h3>No saved conversations</h3>
              <p>Your first local conversation will appear here.</p>
            </div>
          ) : (
            visibleSessions.map((session) => (
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

          <SignalRow label="R2H Intelligence Core" value={khojRuntimeLabel(khojRuntime)} />

          <SignalRow label="Qwen3" value={localModelRuntimeLabel(localModelRuntime)} />

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

function ModelsScreen({
  khojRuntime,
  khojError,
  khojActionPending,
  onKhojAction,
  localModelRuntime,
  localModelError,
  localModelActionPending,
  onLocalModelAction,
}: {
  khojRuntime: KhojRuntimeStatusDto;
  khojError: string;
  khojActionPending: boolean;
  onKhojAction: (action: "start" | "stop" | "restart") => void;
  localModelRuntime: LocalModelRuntimeStatusDto;
  localModelError: string;
  localModelActionPending: boolean;
  onLocalModelAction: (action: "start" | "stop" | "restart") => void;
}) {
  const [filter, setFilter] = useState("All");
  const roles = ["All", "Chat", "Embeddings", "Reranking"];

  const models = [
    {
      role: "Chat",
      name: "Qwen3-4B",
      detail:
        localModelRuntime.state === "Ready"
          ? `Local generation endpoint: ${localModelRuntime.endpoint}`
          : (localModelRuntime.last_error ?? "Verified Qwen3 GGUF model managed by R2H."),
      installed: true,
      state: localModelRuntime.state,
      size: "2.33 GiB",
      format: "GGUF · Q4_K_M",
      file: modelFileName(localModelRuntime.model_path) || "Qwen3-4B-Q4_K_M.gguf",
    },
    {
      role: "Embeddings",
      name: "Qwen3 Embedding 0.6B",
      detail: "Model asset detected. Runtime integration is not active yet.",
      installed: true,
      state: "Stopped",
      size: "609 MiB",
      format: "GGUF · Q8_0",
      file: "Qwen3-Embedding-0.6B-Q8_0.gguf",
    },
    {
      role: "Reranking",
      name: "Qwen3 Reranker 0.6B",
      detail: "Model asset detected. Runtime integration is not active yet.",
      installed: true,
      state: "Stopped",
      size: "461 MiB",
      format: "GGUF · Q4_K_M",
      file: "Qwen3-Reranker-0.6B.Q4_K_M.gguf",
    },
  ].filter((model) => filter === "All" || model.role === filter);

  const loadedModels = localModelRuntime.state === "Ready" ? 1 : 0;

  return (
    <div className="stack">
      <PageHeader
        title="Local Models"
        description="A private control center for verified model packs and runtime resources."
      />

      <section className="model-command glass-panel">
        <div>
          <span className="command-icon">
            <Icon name="model" />
          </span>
          <div>
            <h2>Qwen3-4B {localModelRuntimeLabel(localModelRuntime).toLowerCase()}</h2>
            <p>
              {localModelRuntime.state === "Ready"
                ? `Local chat model is listening at ${localModelRuntime.endpoint}.`
                : (localModelRuntime.last_error ??
                  "The local Qwen generation runtime is managed by R2H.")}
            </p>
          </div>
        </div>

        <div>
          <StatusPill tone={localModelRuntimeTone(localModelRuntime)}>
            {localModelRuntimeLabel(localModelRuntime)}
          </StatusPill>

          {localModelRuntime.state === "Stopped" || localModelRuntime.state === "Failed" ? (
            <button
              className="primary-button"
              type="button"
              disabled={localModelActionPending}
              onClick={() => onLocalModelAction("start")}
            >
              {localModelActionPending ? "Starting…" : "Start"}
            </button>
          ) : (
            <>
              <button
                className="quiet-button"
                type="button"
                disabled={localModelActionPending || localModelRuntime.state !== "Ready"}
                onClick={() => onLocalModelAction("restart")}
              >
                Restart
              </button>
              <button
                className="quiet-button"
                type="button"
                disabled={localModelActionPending || localModelRuntime.state === "Stopping"}
                onClick={() => onLocalModelAction("stop")}
              >
                Stop
              </button>
            </>
          )}
        </div>

        {localModelError && <ErrorPanel message={localModelError} />}
      </section>

      <section className="model-command glass-panel">
        <div>
          <span className="command-icon">
            <Icon name="model" />
          </span>
          <div>
            <h2>Khoj runtime {khojRuntimeLabel(khojRuntime).toLowerCase()}</h2>
            <p>
              {khojRuntime.state === "Ready"
                ? `Local semantic and agent service: ${khojRuntime.endpoint}.`
                : (khojRuntime.last_error ??
                  "The local semantic and agent service is managed by R2H.")}
            </p>
          </div>
        </div>

        <div>
          <StatusPill tone={khojRuntimeTone(khojRuntime)}>
            {khojRuntimeLabel(khojRuntime)}
          </StatusPill>

          {khojRuntime.state === "Stopped" || khojRuntime.state === "Failed" ? (
            <button
              className="primary-button"
              type="button"
              disabled={khojActionPending}
              onClick={() => onKhojAction("start")}
            >
              {khojActionPending ? "Starting…" : "Start"}
            </button>
          ) : (
            <>
              <button
                className="quiet-button"
                type="button"
                disabled={khojActionPending || khojRuntime.state !== "Ready"}
                onClick={() => onKhojAction("restart")}
              >
                Restart
              </button>
              <button
                className="quiet-button"
                type="button"
                disabled={khojActionPending || khojRuntime.state === "Stopping"}
                onClick={() => onKhojAction("stop")}
              >
                Stop
              </button>
            </>
          )}
        </div>

        {khojError && <ErrorPanel message={khojError} />}
      </section>

      <div className="segmented-control" role="group" aria-label="Model role filter">
        {roles.map((role) => (
          <button
            key={role}
            type="button"
            className={filter === role ? "active" : ""}
            onClick={() => setFilter(role)}
          >
            {role}
          </button>
        ))}
      </div>

      <div className="model-layout">
        <section className="model-grid">
          {models.map((model) => (
            <article className="glass-panel model-card" key={model.role}>
              <div className="model-card-head">
                <span>
                  <Icon name="model" />
                </span>

                <StatusPill
                  tone={
                    model.role === "Chat"
                      ? localModelRuntimeTone(localModelRuntime)
                      : "neutral"
                  }
                >
                  {model.role === "Chat"
                    ? localModelRuntimeLabel(localModelRuntime)
                    : "Installed"}
                </StatusPill>
              </div>

              <small>{model.role}</small>
              <h2>{model.name}</h2>
              <p>{model.detail}</p>

              <dl>
                <div>
                  <dt>Size</dt>
                  <dd>{model.size}</dd>
                </div>
                <div>
                  <dt>Format</dt>
                  <dd>{model.format}</dd>
                </div>
                <div>
                  <dt>Role</dt>
                  <dd>{model.role}</dd>
                </div>
              </dl>

              <small>{model.file}</small>
            </article>
          ))}
        </section>

        <aside className="glass-panel performance-panel">
          <PanelHeading title="Runtime status" meta="Live local process state" />

          <div className="empty-meter">
            <span>{loadedModels}</span>
          </div>

          <SignalRow label="Loaded models" value={loadedModels.toString()} />
          <SignalRow
            label="Qwen process"
            value={
              localModelRuntime.pid
                ? `PID ${localModelRuntime.pid}`
                : localModelRuntime.state
            }
          />
          <SignalRow label="Chat endpoint" value={localModelRuntime.endpoint} />
          <SignalRow label="Model identifier" value={localModelRuntime.model_id} />
        </aside>
      </div>
    </div>
  );
}

function BackupScreen({ workspace }: { workspace: WorkspaceDto | null }) {
  return (
    <div className="stack">
      <PageHeader
        title="Backup & Restore"
        description="Create verifiable local snapshots and restore them with confidence."
      />
      <div className="backup-hero">
        <section className="glass-panel backup-action">
          <span className="large-action-icon">
            <Icon name="backup" />
          </span>
          <div>
            <h2>Create a local backup</h2>
            <p>
              Snapshot {workspace?.name ?? "a selected workspace"} with its database, source
              records, and audit chain.
            </p>
          </div>
          <button className="primary-button" type="button" disabled>
            Create backup
          </button>
          <small>Backup commands are not connected in this desktop build.</small>
        </section>
        <section className="glass-panel backup-action restore">
          <span className="large-action-icon">
            <Icon name="import" />
          </span>
          <div>
            <h2>Restore a verified snapshot</h2>
            <p>Inspect a local backup before any workspace data is restored.</p>
          </div>
          <button className="quiet-button" type="button" disabled>
            Choose backup
          </button>
          <small>No restore command is available.</small>
        </section>
      </div>
      <div className="backup-details">
        <section className="glass-panel">
          <PanelHeading title="Verification results" meta="No backup selected" />
          <div className="verification-placeholder">
            <Icon name="integrity" />
            <h3>Awaiting a snapshot</h3>
            <p>Manifest, file hashes, and audit continuity will appear here.</p>
          </div>
        </section>
        <section className="glass-panel">
          <PanelHeading title="Backup history" meta="Local snapshots only" />
          <EmptyState
            compact
            title="No backup history"
            body="Verified snapshots will be listed here after backup commands are connected."
          />
        </section>
      </div>
    </div>
  );
}

function IntegrityScreen({
  workspace,
  integrity,
  loading,
  error,
  eventCount,
}: {
  workspace: WorkspaceDto | null;
  integrity: IntegrityVerificationDto | null;
  loading: boolean;
  error: string;
  eventCount: number;
}) {
  if (!workspace) {
    return (
      <div className="stack">
        <PageHeader
          title="Integrity Check"
          description="Cryptographic confidence for stored knowledge and append-only history."
        />
        <EmptyState
          title="Select a workspace to verify its integrity"
          body="Every verification is scoped to one isolated local workspace."
        />
      </div>
    );
  }
  const state = loading
    ? "Verifying"
    : integrity?.isValid
      ? "Verified"
      : integrity
        ? "Failed"
        : "Unverified";
  return (
    <div className="stack">
      <PageHeader
        title="Integrity Check"
        description="Cryptographic confidence for stored knowledge and append-only audit history."
      />
      {error && <ErrorPanel message={error} />}
      <section className="integrity-hero glass-panel">
        <div className={`integrity-seal ${integrity?.isValid ? "verified" : ""}`}>
          <Icon name="integrity" />
        </div>
        <div>
          <span>Workspace confidence</span>
          <h2>{state}</h2>
          <p>
            {integrity
              ? integrity.isValid
                ? `The local core verified ${integrity.eventCount} chained events.`
                : `Verification stopped at sequence ${integrity.firstFailure?.sequence ?? "unknown"}.`
              : "No current verification result is available."}
          </p>
        </div>
        <StatusPill
          tone={integrity?.isValid ? "success" : integrity ? "danger" : "neutral"}
        >
          {state}
        </StatusPill>
      </section>
      <div className="integrity-grid">
        <HealthCard
          icon="database"
          label="Stored knowledge"
          value={`${eventCount} events`}
          detail="Workspace-scoped records"
          onClick={() => undefined}
        />
        <HealthCard
          icon="integrity"
          label="Audit chain"
          value={state}
          detail="SHA-256 linked history"
          onClick={() => undefined}
        />
        <HealthCard
          icon="archive"
          label="Scan history"
          value={integrity ? "Current" : "None"}
          detail="Live desktop verification"
          onClick={() => undefined}
        />
      </div>
      <section className="glass-panel verification-detail">
        <PanelHeading title="Verification details" meta={workspace.name} />
        <SignalRow
          label="Workspace ID"
          value={<span className="mono">{workspace.id}</span>}
        />
        <SignalRow label="Events checked" value={integrity?.eventCount ?? "—"} />
        <SignalRow label="Chain state" value={state} />
        <SignalRow
          label="First failure"
          value={integrity?.firstFailure?.kind ?? "None recorded"}
        />
      </section>
    </div>
  );
}

function SettingsScreen({
  khojRuntime,
  localModelRuntime,
}: {
  khojRuntime: KhojRuntimeStatusDto;
  localModelRuntime: LocalModelRuntimeStatusDto;
}) {
  const [ambientLight, setAmbientLight] = useState(true);
  const [compactRows, setCompactRows] = useState(false);

  return (
    <div className="settings-layout">
      <PageHeader
        title="Settings"
        description="Control this device’s private knowledge environment."
      />
      <aside className="settings-index glass-panel">
        {[
          "General",
          "Appearance",
          "Data & storage",
          "Privacy",
          "Model runtime",
          "Backup",
        ].map((item, index) => (
          <a href={`#setting-${index}`} key={item} className={index === 0 ? "active" : ""}>
            {item}
            <Icon name="chevron" />
          </a>
        ))}
      </aside>
      <div className="settings-content">
        <section className="glass-panel settings-section" id="setting-0">
          <PanelHeading title="General" meta="Desktop behavior" />
          <SettingRow
            title="Launch workspace"
            description="Open the last selected local workspace."
          >
            <select aria-label="Launch workspace">
              <option>Last selected</option>
            </select>
          </SettingRow>
          <SettingRow title="Language" description="Interface language for this device.">
            <select aria-label="Language">
              <option>English</option>
            </select>
          </SettingRow>
        </section>
        <section className="glass-panel settings-section" id="setting-1">
          <PanelHeading title="Appearance" meta="Black, white, and purple" />
          <ToggleRow
            title="Ambient lighting"
            description="Keep the restrained purple depth light active."
            checked={ambientLight}
            onChange={setAmbientLight}
          />
          <ToggleRow
            title="Compact data rows"
            description="Increase information density in source and audit lists."
            checked={compactRows}
            onChange={setCompactRows}
          />
        </section>
        <section className="glass-panel settings-section" id="setting-2">
          <PanelHeading title="Data & storage" meta="Local device only" />
          <SettingRow
            title="Knowledge location"
            description="Managed by the secure desktop core."
          >
            <span className="setting-value">Application data</span>
          </SettingRow>
          <SettingRow
            title="Storage usage"
            description="Live capacity metrics are not exposed."
          >
            <span className="setting-value">Unavailable</span>
          </SettingRow>
        </section>
        <section className="glass-panel settings-section" id="setting-3">
          <PanelHeading title="Privacy" meta="Fail-closed local policy" />
          <SettingRow
            title="Local-only mode"
            description="External services are not configured."
          >
            <StatusPill tone="success">Locked on</StatusPill>
          </SettingRow>
          <SettingRow title="Telemetry" description="No telemetry command is configured.">
            <StatusPill tone="neutral">Unavailable</StatusPill>
          </SettingRow>
        </section>
        <section className="glass-panel settings-section" id="setting-4">
          <PanelHeading
            title="Local AI runtime"
            meta={`R2H Core · ${khojRuntimeLabel(khojRuntime)} · Qwen3 · ${localModelRuntimeLabel(localModelRuntime)}`}
          />
          <SettingRow title="Khoj service" description={khojRuntime.endpoint}>
            <StatusPill tone={khojRuntimeTone(khojRuntime)}>
              {khojRuntimeLabel(khojRuntime)}
            </StatusPill>
          </SettingRow>
          <SettingRow
            title="Default chat model"
            description={`${localModelRuntime.model_id} · ${localModelRuntime.endpoint}`}
          >
            <StatusPill tone={localModelRuntimeTone(localModelRuntime)}>
              {localModelRuntimeLabel(localModelRuntime)}
            </StatusPill>
          </SettingRow>
        </section>
        <section className="glass-panel settings-section" id="setting-5">
          <PanelHeading title="Backup" meta="Commands unavailable" />
          <SettingRow
            title="Automatic backups"
            description="Requires a connected local backup command."
          >
            <StatusPill tone="neutral">Off</StatusPill>
          </SettingRow>
        </section>
      </div>
    </div>
  );
}

function SettingRow({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <div className="setting-row">
      <div>
        <strong>{title}</strong>
        <p>{description}</p>
      </div>
      {children}
    </div>
  );
}

function ToggleRow({
  title,
  description,
  checked,
  onChange,
}: {
  title: string;
  description: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <SettingRow title={title} description={description}>
      <label className="toggle">
        <input
          type="checkbox"
          checked={checked}
          onChange={(event) => onChange(event.target.checked)}
        />
        <span />
      </label>
    </SettingRow>
  );
}

function SearchScreen({
  workspace,
  sources,
  query,
  hits,
  searching,
  error,
  citation,
  citationLoadingId,
  onQueryChange,
  onSubmit,
  onOpenEvidence,
  onCloseCitation,
}: {
  workspace: WorkspaceDto | null;
  sources: SourceDto[];
  query: string;
  hits: SearchHitDto[];
  searching: boolean;
  error: string;
  citation: CitationDto | null;
  citationLoadingId: string;
  onQueryChange: (value: string) => void;
  onSubmit: (event: React.FormEvent) => void;
  onOpenEvidence: (hit: SearchHitDto) => void;
  onCloseCitation: () => void;
}) {
  const sourceNames = new Map(sources.map((source) => [source.id, source.displayName]));
  return (
    <div className="stack">
      <PageHeader
        title="Search"
        description="Retrieve precise local evidence with source-level traceability."
      />
      <div className="search-layout">
        <div className="search-column">
          <section className="search-hero">
            <h2>Search your local knowledge</h2>
            <p>
              Results come from the selected workspace and resolve through stored citations
              before evidence is displayed.
            </p>
            <form className="search-form" onSubmit={onSubmit}>
              <label>
                <span className="sr-only">Search query</span>
                <input
                  aria-label="Search query"
                  value={query}
                  onChange={(event) => onQueryChange(event.target.value)}
                  placeholder="Search indexed knowledge…"
                  disabled={!workspace}
                />
              </label>
              <button
                className="primary-button"
                type="submit"
                disabled={!workspace || !query.trim() || searching}
              >
                {searching ? "Searching…" : "Search"}
              </button>
            </form>
            {!workspace ? (
              <p className="form-guidance">Select a workspace to search.</p>
            ) : !query.trim() && hits.length === 0 ? (
              <p className="form-guidance">Enter a search query to begin.</p>
            ) : null}
          </section>
          {error && <ErrorPanel message={error} />}
          <section className="search-results">
            <div className="section-heading">
              <div>
                <span className="kicker">Ranked evidence</span>
                <h2>Results</h2>
              </div>
              <span className="count">{hits.length}</span>
            </div>
            {hits.length === 0 ? (
              <EmptyState
                compact
                title="Ready for a local search"
                body="Enter a specific phrase to find stored passages with traceable source spans."
              />
            ) : (
              <div className="hit-list">
                {hits.map((hit, index) => (
                  <article key={hit.blockId}>
                    <div className="hit-rank">{String(index + 1).padStart(2, "0")}</div>
                    <div className="hit-body">
                      <div className="hit-meta">
                        <strong>{sourceNames.get(hit.sourceId) ?? hit.sourceId}</strong>
                        <span>{spanLabel(hit.span)}</span>
                        <span>{Math.round(hit.score * 100)}% score</span>
                      </div>
                      <p>{hit.snippet}</p>
                      <button
                        className="text-button"
                        type="button"
                        onClick={() => onOpenEvidence(hit)}
                        disabled={citationLoadingId === hit.blockId}
                      >
                        {citationLoadingId === hit.blockId
                          ? "Resolving evidence…"
                          : "Open evidence"}
                      </button>
                    </div>
                  </article>
                ))}
              </div>
            )}
          </section>
        </div>
        <aside className="glass-panel search-scope-panel">
          <PanelHeading title="Search scope" meta="Selected workspace only" />
          <SignalRow label="Workspace" value={workspace?.name ?? "None"} />
          <SignalRow label="Indexed sources" value={sources.length} />
          <SignalRow label="Result limit" value="25" />
          <div className="drawer-section">
            <h3>Citation preview</h3>
            <p>
              Open a result to resolve its stored excerpt, canonical locator, and immutable
              content hash.
            </p>
          </div>
        </aside>
      </div>
      {citation && (
        <div className="dialog-backdrop">
          <dialog open aria-label="Resolved evidence">
            <div className="drawer-heading">
              <div>
                <span className="kicker">Resolved citation</span>
                <h2>{citation.displayName}</h2>
              </div>
              <button type="button" className="quiet-button" onClick={onCloseCitation}>
                Close
              </button>
            </div>
            <div className="citation-proof">
              <span>{spanLabel(citation.span)}</span>
              <span className="mono">{hashPrefix(citation.contentSha256)}</span>
            </div>
            <blockquote>{citation.excerpt}</blockquote>
            <dl className="source-metadata">
              <div className="wide">
                <dt>Canonical locator</dt>
                <dd className="mono">{citation.canonicalLocator}</dd>
              </div>
            </dl>
          </dialog>
        </div>
      )}
    </div>
  );
}

function AuditScreen({
  workspace,
  events,
  integrity,
  loading,
  error,
}: {
  workspace: WorkspaceDto | null;
  events: AuditEventDto[];
  integrity: IntegrityVerificationDto | null;
  loading: boolean;
  error: string;
}) {
  const [eventFilter, setEventFilter] = useState("");
  const [selectedEvent, setSelectedEvent] = useState<AuditEventDto | null>(null);
  const visibleEvents = events.filter((event) =>
    `${event.eventType} ${event.actor} ${event.sequence}`
      .toLowerCase()
      .includes(eventFilter.toLowerCase()),
  );

  if (!workspace) {
    return (
      <div className="stack">
        <PageHeader
          title="Audit Logs"
          description="Inspect append-only local events and their cryptographic chain."
        />
        <EmptyState
          title="Select a workspace to inspect its audit chain"
          body="Each workspace has an isolated, append-only event history."
        />
      </div>
    );
  }
  return (
    <div className="stack">
      <PageHeader
        title="Audit Logs"
        description={`Append-only local events for ${workspace.name}.`}
        action={
          integrity && (
            <StatusPill tone={integrity.isValid ? "success" : "danger"}>
              {integrity.isValid ? "Chain verified" : "Verification failed"}
            </StatusPill>
          )
        }
      />
      {error && <ErrorPanel message={error} />}
      {integrity && !integrity.isValid && integrity.firstFailure && (
        <ErrorPanel
          message={`Chain verification failed at sequence ${integrity.firstFailure.sequence}.`}
        />
      )}
      <div className="filter-bar audit-filter">
        <label className="filter-search">
          <Icon name="search" />
          <input
            aria-label="Filter audit logs"
            value={eventFilter}
            onChange={(event) => setEventFilter(event.target.value)}
            placeholder="Search event, actor, or sequence…"
          />
        </label>
        <button className="quiet-button" type="button">
          All actors
        </button>
        <span className="count">{visibleEvents.length}</span>
      </div>
      <div className="audit-layout">
        <section className="glass-panel audit-timeline">
          <PanelHeading title="Event timeline" meta="Newest verified record first" />
          {loading ? (
            <p className="loading-copy">Verifying audit history…</p>
          ) : visibleEvents.length === 0 ? (
            <EmptyState
              compact
              title={events.length === 0 ? "No audit events" : "No matching events"}
              body={
                events.length === 0
                  ? "New local actions will appear here in sequence."
                  : "Adjust the current audit filter."
              }
            />
          ) : (
            <div className="timeline-list">
              {visibleEvents.map((event) => (
                <button
                  type="button"
                  key={event.id}
                  className={selectedEvent?.id === event.id ? "selected" : ""}
                  onClick={() => setSelectedEvent(event)}
                >
                  <span className="timeline-node">
                    <Icon name="log" />
                  </span>
                  <span>
                    <small>Sequence #{event.sequence}</small>
                    <strong>{event.eventType}</strong>
                    <em>
                      {event.actor} · {formatLocal(event.occurredAtUtc)}
                    </em>
                  </span>
                  <span className="timeline-hash mono">{hashPrefix(event.eventHash)}</span>
                  <Icon name="chevron" />
                </button>
              ))}
            </div>
          )}
        </section>
        <aside className="glass-panel audit-detail">
          {selectedEvent ? (
            <>
              <PanelHeading
                title="Event detail"
                action={
                  <button
                    type="button"
                    className="text-button"
                    onClick={() => setSelectedEvent(null)}
                  >
                    Close
                  </button>
                }
              />
              <span className="event-sequence">#{selectedEvent.sequence}</span>
              <h2>{selectedEvent.eventType}</h2>
              <div className="trust-lines">
                <SignalRow label="Actor" value={selectedEvent.actor} />
                <SignalRow label="UTC" value={formatUtc(selectedEvent.occurredAtUtc)} />
                <SignalRow
                  label="Local time"
                  value={formatLocal(selectedEvent.occurredAtUtc)}
                />
                <SignalRow
                  label="Chain"
                  value={integrity?.isValid ? "Verified" : "Unverified"}
                />
              </div>
              <div className="hash-block">
                <span>Event hash</span>
                <code>{selectedEvent.eventHash}</code>
              </div>
            </>
          ) : (
            <div className="detail-placeholder">
              <span>
                <Icon name="log" />
              </span>
              <h2>Inspect an event</h2>
              <p>Select a timeline record to view its exact timestamp, actor, and hash.</p>
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}
