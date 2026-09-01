from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(r"E:\Projects\r2h-second-brain")
FRONTEND = ROOT / "apps" / "desktop" / "src"

CONTRACTS = FRONTEND / "api" / "contracts.ts"
CLIENT = FRONTEND / "api" / "client.ts"
APP = FRONTEND / "App.tsx"


def backup(path: Path) -> None:
    target = path.with_name(path.name + ".before-phase-c2-qwen-ui")
    if not target.exists():
        target.write_bytes(path.read_bytes())


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{label}: expected one match, found {count}")
    return text.replace(old, new, 1)


# ============================================================
# contracts.ts
# ============================================================

backup(CONTRACTS)
contracts = CONTRACTS.read_text(encoding="utf-8")

contract_marker = '''export interface KhojRuntimeStatusDto {
  state: KhojRuntimeState;
  pid: number | null;
  endpoint: string;
  started_at: string | null;
  last_error: string | null;
}
'''

contract_addition = contract_marker + '''
export type LocalModelRuntimeState =
  | "Stopped"
  | "Starting"
  | "Ready"
  | "Failed"
  | "Stopping";

export interface LocalModelRuntimeStatusDto {
  state: LocalModelRuntimeState;
  pid: number | null;
  endpoint: string;
  model_id: string;
  model_path: string;
  started_at: string | null;
  last_error: string | null;
}
'''

contracts = replace_once(
    contracts,
    contract_marker,
    contract_addition,
    "local model runtime contract",
)

CONTRACTS.write_text(contracts, encoding="utf-8", newline="\n")


# ============================================================
# client.ts
# ============================================================

backup(CLIENT)
client = CLIENT.read_text(encoding="utf-8")

client = replace_once(
    client,
    '''  KhojRuntimeStatusDto,
  SearchHitDto,''',
    '''  KhojRuntimeStatusDto,
  LocalModelRuntimeStatusDto,
  SearchHitDto,''',
    "client type import",
)

client_marker = '''export async function restartKhojRuntime(): Promise<KhojRuntimeStatusDto> {
  return invoke<KhojRuntimeStatusDto>("khoj_runtime_restart");
}
'''

client_addition = client_marker + '''
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
'''

client = replace_once(
    client,
    client_marker,
    client_addition,
    "client local model commands",
)

CLIENT.write_text(client, encoding="utf-8", newline="\n")


# ============================================================
# App.tsx — imports and helpers
# ============================================================

backup(APP)
app = APP.read_text(encoding="utf-8")

app = replace_once(
    app,
    '''  KhojRuntimeStatusDto,
  SearchHitDto,''',
    '''  KhojRuntimeStatusDto,
  LocalModelRuntimeStatusDto,
  SearchHitDto,''',
    "App type import",
)

helper_marker = '''const INITIAL_KHOJ_RUNTIME: KhojRuntimeStatusDto = {
  state: "Stopped",
  pid: null,
  endpoint: "http://127.0.0.1:42110",
  started_at: null,
  last_error: null,
};
'''

helper_addition = helper_marker + '''
const INITIAL_LOCAL_MODEL_RUNTIME: LocalModelRuntimeStatusDto = {
  state: "Stopped",
  pid: null,
  endpoint: "http://127.0.0.1:42111",
  model_id: "qwen3-4b-r2h",
  model_path: "",
  started_at: null,
  last_error: null,
};
'''

app = replace_once(
    app,
    helper_marker,
    helper_addition,
    "initial local model status",
)

tone_marker = '''function khojRuntimeTone(
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
'''

tone_addition = tone_marker + '''
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
  return path.split(/[\\\\/]/).filter(Boolean).at(-1) ?? path;
}
'''

app = replace_once(
    app,
    tone_marker,
    tone_addition,
    "local model status helpers",
)


# ============================================================
# App.tsx — state
# ============================================================

state_marker = '''  const [khojRuntimeAction, setKhojRuntimeAction] = useState(false);
'''

state_addition = state_marker + '''
  const [localModelRuntime, setLocalModelRuntime] =
    useState<LocalModelRuntimeStatusDto>(INITIAL_LOCAL_MODEL_RUNTIME);
  const [localModelRuntimeError, setLocalModelRuntimeError] = useState("");
  const [localModelRuntimeAction, setLocalModelRuntimeAction] = useState(false);
'''

app = replace_once(
    app,
    state_marker,
    state_addition,
    "local model state",
)


# ============================================================
# App.tsx — polling and actions
# ============================================================

action_end_marker = '''  async function runKhojRuntimeAction(
    action: "start" | "stop" | "restart",
  ) {
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
'''

local_runtime_logic = action_end_marker + '''
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

  async function runLocalModelRuntimeAction(
    action: "start" | "stop" | "restart",
  ) {
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
'''

app = replace_once(
    app,
    action_end_marker,
    local_runtime_logic,
    "local model polling and actions",
)


# ============================================================
# App.tsx — component wiring
# ============================================================

app = replace_once(
    app,
    '''              <DashboardScreen
                workspaces={workspaces}
                selectedWorkspace={selectedWorkspace}
                sources={sources}
                integrity={integrity}
                khojRuntime={khojRuntime}
                onNavigate={navigate}''',
    '''              <DashboardScreen
                workspaces={workspaces}
                selectedWorkspace={selectedWorkspace}
                sources={sources}
                integrity={integrity}
                khojRuntime={khojRuntime}
                localModelRuntime={localModelRuntime}
                onNavigate={navigate}''',
    "dashboard local model wiring",
)

app = replace_once(
    app,
    '''              <ModelsScreen
                runtime={khojRuntime}
                error={khojRuntimeError}
                actionPending={khojRuntimeAction}
                onAction={runKhojRuntimeAction}
              />''',
    '''              <ModelsScreen
                khojRuntime={khojRuntime}
                khojError={khojRuntimeError}
                khojActionPending={khojRuntimeAction}
                onKhojAction={runKhojRuntimeAction}
                localModelRuntime={localModelRuntime}
                localModelError={localModelRuntimeError}
                localModelActionPending={localModelRuntimeAction}
                onLocalModelAction={runLocalModelRuntimeAction}
              />''',
    "ModelsScreen wiring",
)

app = replace_once(
    app,
    '''              <SettingsScreen runtime={khojRuntime} />''',
    '''              <SettingsScreen
                khojRuntime={khojRuntime}
                localModelRuntime={localModelRuntime}
              />''',
    "SettingsScreen wiring",
)


# ============================================================
# App.tsx — Dashboard
# ============================================================

app = replace_once(
    app,
    '''  khojRuntime,
  onNavigate,''',
    '''  khojRuntime,
  localModelRuntime,
  onNavigate,''',
    "dashboard arguments",
)

app = replace_once(
    app,
    '''  khojRuntime: KhojRuntimeStatusDto;
  onNavigate: (screen: Screen) => void;''',
    '''  khojRuntime: KhojRuntimeStatusDto;
  localModelRuntime: LocalModelRuntimeStatusDto;
  onNavigate: (screen: Screen) => void;''',
    "dashboard argument types",
)

app = replace_once(
    app,
    '''                meta={`Khoj service · ${khojRuntimeLabel(khojRuntime)}`}''',
    '''                meta={`Khoj · ${khojRuntimeLabel(khojRuntime)} · Qwen3 · ${localModelRuntimeLabel(localModelRuntime)}`}''',
    "dashboard assistant status",
)


# ============================================================
# App.tsx — replace ModelsScreen
# ============================================================

new_models_screen = r'''function ModelsScreen({
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
          : localModelRuntime.last_error ??
            "Verified Qwen3 GGUF model managed by R2H.",
      installed: true,
      state: localModelRuntime.state,
      size: "2.33 GiB",
      format: "GGUF · Q4_K_M",
      file:
        modelFileName(localModelRuntime.model_path) ||
        "Qwen3-4B-Q4_K_M.gguf",
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
                : localModelRuntime.last_error ??
                  "The local Qwen generation runtime is managed by R2H."}
            </p>
          </div>
        </div>

        <div>
          <StatusPill tone={localModelRuntimeTone(localModelRuntime)}>
            {localModelRuntimeLabel(localModelRuntime)}
          </StatusPill>

          {localModelRuntime.state === "Stopped" ||
          localModelRuntime.state === "Failed" ? (
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
                disabled={
                  localModelActionPending ||
                  localModelRuntime.state !== "Ready"
                }
                onClick={() => onLocalModelAction("restart")}
              >
                Restart
              </button>
              <button
                className="quiet-button"
                type="button"
                disabled={
                  localModelActionPending ||
                  localModelRuntime.state === "Stopping"
                }
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
                : khojRuntime.last_error ??
                  "The local semantic and agent service is managed by R2H."}
            </p>
          </div>
        </div>

        <div>
          <StatusPill tone={khojRuntimeTone(khojRuntime)}>
            {khojRuntimeLabel(khojRuntime)}
          </StatusPill>

          {khojRuntime.state === "Stopped" ||
          khojRuntime.state === "Failed" ? (
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
                disabled={
                  khojActionPending || khojRuntime.state === "Stopping"
                }
                onClick={() => onKhojAction("stop")}
              >
                Stop
              </button>
            </>
          )}
        </div>

        {khojError && <ErrorPanel message={khojError} />}
      </section>

      <div
        className="segmented-control"
        role="group"
        aria-label="Model role filter"
      >
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
          <PanelHeading
            title="Runtime status"
            meta="Live local process state"
          />

          <div className="empty-meter">
            <span>{loadedModels}</span>
          </div>

          <SignalRow
            label="Loaded models"
            value={loadedModels.toString()}
          />
          <SignalRow
            label="Qwen process"
            value={
              localModelRuntime.pid
                ? `PID ${localModelRuntime.pid}`
                : localModelRuntime.state
            }
          />
          <SignalRow
            label="Chat endpoint"
            value={localModelRuntime.endpoint}
          />
          <SignalRow
            label="Model identifier"
            value={localModelRuntime.model_id}
          />
        </aside>
      </div>
    </div>
  );
}
'''

models_pattern = re.compile(
    r'function ModelsScreen\(\{.*?\n\}\n\nfunction BackupScreen',
    re.DOTALL,
)

match = models_pattern.search(app)
if not match:
    raise RuntimeError("ModelsScreen block was not found")

app = (
    app[: match.start()]
    + new_models_screen
    + "\n\nfunction BackupScreen"
    + app[match.end() :]
)


# ============================================================
# App.tsx — SettingsScreen props and runtime section
# ============================================================

app = replace_once(
    app,
    '''function SettingsScreen({
  runtime,
}: {
  runtime: KhojRuntimeStatusDto;
}) {''',
    '''function SettingsScreen({
  khojRuntime,
  localModelRuntime,
}: {
  khojRuntime: KhojRuntimeStatusDto;
  localModelRuntime: LocalModelRuntimeStatusDto;
}) {''',
    "SettingsScreen props",
)

settings_old = '''          <PanelHeading
            title="Khoj runtime"
            meta={`Local service · ${khojRuntimeLabel(runtime)}`}
          />
          <SettingRow
            title="Service state"
            description={runtime.endpoint}
          >
            <StatusPill tone={khojRuntimeTone(runtime)}>
              {khojRuntimeLabel(runtime)}
            </StatusPill>
          </SettingRow>
          <SettingRow
            title="Default chat model"
            description="Model-pack integration is handled in the next phase."
          >
            <button className="quiet-button" type="button" disabled>
              Choose model
            </button>
          </SettingRow>'''

settings_new = '''          <PanelHeading
            title="Local AI runtime"
            meta={`Khoj · ${khojRuntimeLabel(khojRuntime)} · Qwen3 · ${localModelRuntimeLabel(localModelRuntime)}`}
          />
          <SettingRow
            title="Khoj service"
            description={khojRuntime.endpoint}
          >
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
          </SettingRow>'''

app = replace_once(
    app,
    settings_old,
    settings_new,
    "Settings runtime section",
)

APP.write_text(app, encoding="utf-8", newline="\n")

print(f"UPDATED {CONTRACTS}")
print(f"UPDATED {CLIENT}")
print(f"UPDATED {APP}")
print("PHASE_C2_QWEN_UI_WRITTEN")
