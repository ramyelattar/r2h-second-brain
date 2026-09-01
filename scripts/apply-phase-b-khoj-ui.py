from __future__ import annotations

from pathlib import Path


ROOT = Path(r"E:\Projects\r2h-second-brain")
LIB = ROOT / "apps" / "desktop" / "src-tauri" / "src" / "lib.rs"
CLIENT = ROOT / "apps" / "desktop" / "src" / "api" / "client.ts"
CONTRACTS = ROOT / "apps" / "desktop" / "src" / "api" / "contracts.ts"
APP = ROOT / "apps" / "desktop" / "src" / "App.tsx"


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(
            f"{label}: expected exactly one match, found {count}"
        )
    return text.replace(old, new, 1)


def write_updated(path: Path, text: str) -> None:
    backup = path.with_name(path.name + ".before-phase-b-khoj-ui")
    if not backup.exists():
        backup.write_text(path.read_text(encoding="utf-8"), encoding="utf-8")
    path.write_text(text, encoding="utf-8", newline="\n")
    print(f"UPDATED {path}")


# ---------------------------------------------------------------------
# Rust / Tauri boundary
# ---------------------------------------------------------------------

lib = LIB.read_text(encoding="utf-8")

lib = replace_once(
    lib,
    'pub const COMMAND_NAMES: [&str; 13] = [',
    'pub const COMMAND_NAMES: [&str; 17] = [',
    "COMMAND_NAMES length",
)

lib = replace_once(
    lib,
    '''    "restore_workspace_backup",
];''',
    '''    "restore_workspace_backup",
    "khoj_runtime_status",
    "khoj_runtime_start",
    "khoj_runtime_stop",
    "khoj_runtime_restart",
];''',
    "Khoj command names",
)

runtime_commands = r'''
fn runtime_status_result(
    runtime: &KhojRuntimeManager,
) -> Result<KhojRuntimeStatus, String> {
    runtime
        .status()
        .map_err(|_| "Khoj runtime status is unavailable".to_owned())
}

#[tauri::command]
fn khoj_runtime_status(
    runtime: tauri::State<'_, KhojRuntimeManager>,
) -> Result<KhojRuntimeStatus, String> {
    runtime_status_result(runtime.inner())
}

#[tauri::command]
fn khoj_runtime_start(
    runtime: tauri::State<'_, KhojRuntimeManager>,
) -> Result<KhojRuntimeStatus, String> {
    let owned_runtime = runtime.inner().clone();
    thread::spawn(move || {
        let _ = owned_runtime.start();
    });

    runtime_status_result(runtime.inner())
}

#[tauri::command]
fn khoj_runtime_stop(
    runtime: tauri::State<'_, KhojRuntimeManager>,
) -> Result<KhojRuntimeStatus, String> {
    let owned_runtime = runtime.inner().clone();
    thread::spawn(move || {
        let _ = owned_runtime.stop();
    });

    runtime_status_result(runtime.inner())
}

#[tauri::command]
fn khoj_runtime_restart(
    runtime: tauri::State<'_, KhojRuntimeManager>,
) -> Result<KhojRuntimeStatus, String> {
    let owned_runtime = runtime.inner().clone();
    thread::spawn(move || {
        if owned_runtime.stop().is_ok() {
            let _ = owned_runtime.start();
        }
    });

    runtime_status_result(runtime.inner())
}

'''

lib = replace_once(
    lib,
    '''pub fn run() -> Result<(), Box<dyn std::error::Error>> {''',
    runtime_commands
    + '''pub fn run() -> Result<(), Box<dyn std::error::Error>> {''',
    "Runtime command insertion",
)

lib = replace_once(
    lib,
    '''            commands::maintenance::tauri_handlers::restore_workspace_backup,
        ])''',
    '''            commands::maintenance::tauri_handlers::restore_workspace_backup,
            khoj_runtime_status,
            khoj_runtime_start,
            khoj_runtime_stop,
            khoj_runtime_restart,
        ])''',
    "Tauri handler registration",
)

write_updated(LIB, lib)


# ---------------------------------------------------------------------
# TypeScript contracts
# ---------------------------------------------------------------------

contracts = CONTRACTS.read_text(encoding="utf-8")

contracts_addition = r'''

export type KhojRuntimeState =
  | "Stopped"
  | "Starting"
  | "Ready"
  | "Failed"
  | "Stopping";

export interface KhojRuntimeStatusDto {
  state: KhojRuntimeState;
  pid: number | null;
  endpoint: string;
  started_at: string | null;
  last_error: string | null;
}
'''

if "export interface KhojRuntimeStatusDto" not in contracts:
    contracts = contracts.rstrip() + contracts_addition + "\n"

write_updated(CONTRACTS, contracts)


# ---------------------------------------------------------------------
# Typed frontend API
# ---------------------------------------------------------------------

client = CLIENT.read_text(encoding="utf-8")

client = replace_once(
    client,
    '''  IntegrityVerificationDto,
  SearchHitDto,''',
    '''  IntegrityVerificationDto,
  KhojRuntimeStatusDto,
  SearchHitDto,''',
    "Client Khoj type import",
)

client_addition = r'''

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
'''

if "export async function getKhojRuntimeStatus" not in client:
    client = client.rstrip() + client_addition + "\n"

write_updated(CLIENT, client)


# ---------------------------------------------------------------------
# React UI
# ---------------------------------------------------------------------

app = APP.read_text(encoding="utf-8")

app = replace_once(
    app,
    '''  IntegrityVerificationDto,
  SearchHitDto,''',
    '''  IntegrityVerificationDto,
  KhojRuntimeStatusDto,
  SearchHitDto,''',
    "App Khoj type import",
)

runtime_helpers = r'''

const INITIAL_KHOJ_RUNTIME: KhojRuntimeStatusDto = {
  state: "Stopped",
  pid: null,
  endpoint: "http://127.0.0.1:42110",
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
'''

app = replace_once(
    app,
    '''function fileName(path: string): string {''',
    runtime_helpers + '''

function fileName(path: string): string {''',
    "Runtime UI helpers",
)

app = replace_once(
    app,
    '''  const [appError, setAppError] = useState("");

  const [workspaceName,''',
    '''  const [appError, setAppError] = useState("");

  const [khojRuntime, setKhojRuntime] =
    useState<KhojRuntimeStatusDto>(INITIAL_KHOJ_RUNTIME);
  const [khojRuntimeError, setKhojRuntimeError] = useState("");
  const [khojRuntimeAction, setKhojRuntimeAction] = useState(false);

  const [workspaceName,''',
    "Runtime React state",
)

runtime_effect = r'''
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

  async function runKhojRuntimeAction(
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

app = replace_once(
    app,
    '''  function changeWorkspace(nextWorkspaceId: string) {''',
    runtime_effect + '''  function changeWorkspace(nextWorkspaceId: string) {''',
    "Runtime polling effect",
)

app = replace_once(
    app,
    '''                integrity={integrity}
                onNavigate={navigate}''',
    '''                integrity={integrity}
                khojRuntime={khojRuntime}
                onNavigate={navigate}''',
    "Dashboard runtime prop call",
)

app = replace_once(
    app,
    '''            {screen === "models" && <ModelsScreen />}''',
    '''            {screen === "models" && (
              <ModelsScreen
                runtime={khojRuntime}
                error={khojRuntimeError}
                actionPending={khojRuntimeAction}
                onAction={runKhojRuntimeAction}
              />
            )}''',
    "ModelsScreen invocation",
)

app = replace_once(
    app,
    '''            {screen === "settings" && <SettingsScreen />}''',
    '''            {screen === "settings" && (
              <SettingsScreen runtime={khojRuntime} />
            )}''',
    "SettingsScreen invocation",
)

app = replace_once(
    app,
    '''  integrity,
  onNavigate,
  onAddFiles,
}: {
  workspaces: WorkspaceDto[];
  selectedWorkspace: WorkspaceDto | null;
  sources: SourceDto[];
  integrity: IntegrityVerificationDto | null;
  onNavigate: (screen: Screen) => void;''',
    '''  integrity,
  khojRuntime,
  onNavigate,
  onAddFiles,
}: {
  workspaces: WorkspaceDto[];
  selectedWorkspace: WorkspaceDto | null;
  sources: SourceDto[];
  integrity: IntegrityVerificationDto | null;
  khojRuntime: KhojRuntimeStatusDto;
  onNavigate: (screen: Screen) => void;''',
    "DashboardScreen signature",
)

app = replace_once(
    app,
    '''              <PanelHeading title="AI assistant" meta="Local runtime not connected" />''',
    '''              <PanelHeading
                title="AI assistant"
                meta={`Khoj service · ${khojRuntimeLabel(khojRuntime)}`}
              />''',
    "Assistant panel runtime label",
)

app = replace_once(
    app,
    '''          <SignalRow label="Model runtime" value="Not connected" />''',
    '''          <SignalRow
            label="Khoj service"
            value={
              <StatusPill tone={khojRuntimeTone(khojRuntime)}>
                {khojRuntimeLabel(khojRuntime)}
              </StatusPill>
            }
          />''',
    "Dashboard system runtime row",
)

old_models = r'''function ModelsScreen() {
  const [filter, setFilter] = useState("All");'''

new_models = r'''function ModelsScreen({
  runtime,
  error,
  actionPending,
  onAction,
}: {
  runtime: KhojRuntimeStatusDto;
  error: string;
  actionPending: boolean;
  onAction: (action: "start" | "stop" | "restart") => void;
}) {
  const [filter, setFilter] = useState("All");'''

app = replace_once(app, old_models, new_models, "ModelsScreen signature")

app = replace_once(
    app,
    '''            <h2>No runtime connected</h2>
            <p>Model discovery and lifecycle commands are not available in this build.</p>
          </div>
        </div>
        <StatusPill tone="neutral">Unavailable</StatusPill>''',
    '''            <h2>Khoj runtime {khojRuntimeLabel(runtime).toLowerCase()}</h2>
            <p>
              {runtime.state === "Ready"
                ? `Local Khoj service is listening at ${runtime.endpoint}.`
                : runtime.last_error ??
                  "The local semantic and agent service is managed by R2H."}
            </p>
          </div>
        </div>
        <div>
          <StatusPill tone={khojRuntimeTone(runtime)}>
            {khojRuntimeLabel(runtime)}
          </StatusPill>
          {runtime.state === "Stopped" || runtime.state === "Failed" ? (
            <button
              className="primary-button"
              type="button"
              disabled={actionPending}
              onClick={() => onAction("start")}
            >
              {actionPending ? "Starting…" : "Start"}
            </button>
          ) : (
            <>
              <button
                className="quiet-button"
                type="button"
                disabled={actionPending || runtime.state !== "Ready"}
                onClick={() => onAction("restart")}
              >
                Restart
              </button>
              <button
                className="quiet-button"
                type="button"
                disabled={actionPending || runtime.state === "Stopping"}
                onClick={() => onAction("stop")}
              >
                Stop
              </button>
            </>
          )}
        </div>
        {error && <ErrorPanel message={error} />}''',
    "Models runtime panel",
)

app = replace_once(
    app,
    '''function SettingsScreen() {
  const [ambientLight,''',
    '''function SettingsScreen({
  runtime,
}: {
  runtime: KhojRuntimeStatusDto;
}) {
  const [ambientLight,''',
    "SettingsScreen signature",
)

app = replace_once(
    app,
    '''          <PanelHeading title="Model runtime" meta="No runtime connected" />
          <SettingRow
            title="Default chat model"
            description="Install and verify a local model pack first."
          >
            <button className="quiet-button" type="button" disabled>
              Choose model
            </button>
          </SettingRow>''',
    '''          <PanelHeading
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
          </SettingRow>''',
    "Settings runtime panel",
)

write_updated(APP, app)

print("PHASE_B_KHOJ_UI_PATCH_COMPLETE")
