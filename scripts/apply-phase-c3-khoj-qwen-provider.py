from __future__ import annotations

from pathlib import Path


ROOT = Path(r"E:\Projects\r2h-second-brain")
LIB = ROOT / "apps" / "desktop" / "src-tauri" / "src" / "lib.rs"
KHOJ_RUNTIME = (
    ROOT
    / "apps"
    / "desktop"
    / "src-tauri"
    / "src"
    / "khoj_runtime.rs"
)
LOCAL_RUNTIME = (
    ROOT
    / "apps"
    / "desktop"
    / "src-tauri"
    / "src"
    / "local_model_runtime.rs"
)


def backup(path: Path) -> None:
    backup_path = path.with_name(path.name + ".before-phase-c3")
    if not backup_path.exists():
        backup_path.write_bytes(path.read_bytes())


def replace_once(
    text: str,
    old: str,
    new: str,
    label: str,
) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(
            f"{label}: expected exactly one match, found {count}"
        )
    return text.replace(old, new, 1)


# ============================================================
# 1. Disable Qwen thinking globally for Khoj requests.
# ============================================================

backup(LOCAL_RUNTIME)
local_runtime = LOCAL_RUNTIME.read_text(encoding="utf-8")

old_args = '''                "--threads",
                &threads.to_string(),
                "--jinja",
            ])'''

new_args = '''                "--threads",
                &threads.to_string(),
                "--jinja",
                "--reasoning",
                "off",
            ])'''

local_runtime = replace_once(
    local_runtime,
    old_args,
    new_args,
    "llama reasoning configuration",
)

LOCAL_RUNTIME.write_text(
    local_runtime,
    encoding="utf-8",
    newline="\n",
)


# ============================================================
# 2. Inject local OpenAI-compatible provider into Khoj.
# ============================================================

backup(KHOJ_RUNTIME)
khoj_runtime = KHOJ_RUNTIME.read_text(encoding="utf-8")

old_command = '''        command
            .arg(wrapper)
            .current_dir(&config.project_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));'''

new_command = '''        command
            .arg(wrapper)
            .current_dir(&config.project_root)
            .env(
                "OPENAI_BASE_URL",
                "http://127.0.0.1:42111/v1/",
            )
            .env("OPENAI_API_KEY", "r2h-local")
            .env(
                "KHOJ_DEFAULT_CHAT_MODEL",
                "qwen3-4b-r2h",
            )
            .env("R2H_KHOJ_LOCAL_MODEL_PROVIDER", "llama.cpp")
            .stdin(Stdio::piped())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));'''

khoj_runtime = replace_once(
    khoj_runtime,
    old_command,
    new_command,
    "Khoj provider environment",
)

KHOJ_RUNTIME.write_text(
    khoj_runtime,
    encoding="utf-8",
    newline="\n",
)


# ============================================================
# 3. Start Qwen first, then start Khoj.
# ============================================================

backup(LIB)
lib = LIB.read_text(encoding="utf-8")

old_setup = '''            let project_root = resolve_project_root()?;
            let khoj_runtime =
                KhojRuntimeManager::new(KhojRuntimeConfig::development(project_root));
            let startup_runtime = khoj_runtime.clone();
            app.manage(khoj_runtime);
            thread::spawn(move || {
                let _ = startup_runtime.start();
            });

            let local_model_runtime = LocalModelRuntimeManager::new(
                LocalModelRuntimeConfig::development(resolve_project_root()?),
            );
            let startup_local_model_runtime = local_model_runtime.clone();
            app.manage(local_model_runtime);

            thread::spawn(move || {
                let _ = startup_local_model_runtime.start();
            });

            Ok(())'''

new_setup = '''            let project_root = resolve_project_root()?;

            let khoj_runtime = KhojRuntimeManager::new(
                KhojRuntimeConfig::development(project_root.clone()),
            );
            let startup_khoj_runtime = khoj_runtime.clone();
            app.manage(khoj_runtime);

            let local_model_runtime = LocalModelRuntimeManager::new(
                LocalModelRuntimeConfig::development(project_root),
            );
            let startup_local_model_runtime =
                local_model_runtime.clone();
            app.manage(local_model_runtime);

            thread::spawn(move || {
                if startup_local_model_runtime.start().is_ok() {
                    let _ = startup_khoj_runtime.start();
                }
            });

            Ok(())'''

if old_setup not in lib:
    # Accept rustfmt-expanded variant.
    old_setup = '''            let project_root = resolve_project_root()?;
            let khoj_runtime =
                KhojRuntimeManager::new(KhojRuntimeConfig::development(project_root));
            let startup_runtime = khoj_runtime.clone();
            app.manage(khoj_runtime);
            thread::spawn(move || {
                let _ = startup_runtime.start();
            });

            let local_model_runtime = LocalModelRuntimeManager::new(
                LocalModelRuntimeConfig::development(resolve_project_root()?),
            );
            let startup_local_model_runtime = local_model_runtime.clone();
            app.manage(local_model_runtime);

            thread::spawn(move || {
                let _ = startup_local_model_runtime.start();
            });

            Ok(())'''

lib = replace_once(
    lib,
    old_setup,
    new_setup,
    "runtime startup ordering",
)

LIB.write_text(
    lib,
    encoding="utf-8",
    newline="\n",
)

print(f"UPDATED {LOCAL_RUNTIME}")
print(f"UPDATED {KHOJ_RUNTIME}")
print(f"UPDATED {LIB}")
print("PHASE_C3_KHOJ_QWEN_PROVIDER_WRITTEN")
