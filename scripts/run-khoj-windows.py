from __future__ import annotations

import os
import subprocess
import sys
import time
import threading
from pathlib import Path
from urllib.parse import unquote, urlparse

import pgserver


PROJECT_ROOT = Path(__file__).resolve().parent.parent
PG_DATA_DIR = PROJECT_ROOT / "run-data" / "khoj" / "postgres"
KHOJ_SOURCE_DIR = PROJECT_ROOT / "third_party" / "khoj"
R2H_KHOJ_PACKAGE_DIR = PROJECT_ROOT / "engines" / "khoj" / "r2h_khoj"
KHOJ_EXE = Path(sys.executable).parent / "khoj.exe"
KHOJ_HOST = "127.0.0.1"
KHOJ_PORT = "42110"

LOCAL_CHAT_MODEL_ID = "qwen3-4b-r2h"
LOCAL_CHAT_MODEL_NAME = "Qwen3-4B"
LOCAL_CHAT_PROVIDER_NAME = "R2H Local llama.cpp"
LOCAL_CHAT_BASE_URL = "http://127.0.0.1:42111/v1/"
LOCAL_CHAT_API_KEY = "r2h-local"


SETTINGS_SOURCE = '''\
"""Windows compatibility settings for embedded Khoj in R2H Second Brain."""

from khoj.app.settings import *  # noqa: F403,F401

# The bundled pgserver runtime on Windows may not include a timezone database
# entry named "UTC". GMT is an equivalent zero-offset timezone and is accepted
# by the embedded PostgreSQL runtime used by R2H.
TIME_ZONE = "GMT"

# Keep timezone-aware datetime behavior enabled. Override the PostgreSQL
# connection timezone to GMT so Django does not request the unavailable
# timezone name "UTC" from the bundled Windows PostgreSQL runtime.
USE_TZ = True
DATABASES["default"]["TIME_ZONE"] = "GMT"  # noqa: F405
'''


def ensure_r2h_settings() -> None:
    """Create the isolated Django settings shim used by the Windows wrapper."""
    R2H_KHOJ_PACKAGE_DIR.mkdir(parents=True, exist_ok=True)

    init_file = R2H_KHOJ_PACKAGE_DIR / "__init__.py"
    settings_file = R2H_KHOJ_PACKAGE_DIR / "settings.py"

    if not init_file.exists():
        init_file.write_text("", encoding="utf-8")

    current = settings_file.read_text(encoding="utf-8") if settings_file.exists() else None
    if current != SETTINGS_SOURCE:
        settings_file.write_text(SETTINGS_SOURCE, encoding="utf-8")


def build_environment(parsed_uri) -> dict[str, str]:
    """Build the child-process environment for Khoj."""
    env = os.environ.copy()

    # The wrapper owns the embedded PostgreSQL process. Disable Khoj's native
    # embedded-DB path because it attempts to use a Unix socket on Windows.
    env["USE_EMBEDDED_DB"] = "false"
    env["POSTGRES_HOST"] = parsed_uri.hostname or "127.0.0.1"
    env["POSTGRES_PORT"] = str(parsed_uri.port or "")
    env["POSTGRES_USER"] = unquote(parsed_uri.username or "postgres")
    env["POSTGRES_PASSWORD"] = unquote(parsed_uri.password or "postgres")
    env["POSTGRES_DB"] = "khoj"

    env["KHOJ_NO_HTTPS"] = "true"
    env["KHOJ_TELEMETRY_DISABLE"] = "true"
    env["KHOJ_DOMAIN"] = "localhost"
    env["KHOJ_ALLOWED_DOMAIN"] = "localhost"

    # Load the R2H Windows compatibility settings before Khoj imports Django.
    env["DJANGO_SETTINGS_MODULE"] = "r2h_khoj.settings"

    settings_parent = str(R2H_KHOJ_PACKAGE_DIR.parent)
    existing_pythonpath = env.get("PYTHONPATH", "")
    env["PYTHONPATH"] = (
        settings_parent
        if not existing_pythonpath
        else f"{settings_parent}{os.pathsep}{existing_pythonpath}"
    )

    return env



def bootstrap_local_chat_model(
    env: dict[str, str],
) -> None:
    """Create and select the embedded Qwen chat model idempotently."""

    bootstrap_source = f"""
import django

django.setup()

from django.core.management import call_command

from khoj.database.adapters import (
    AgentAdapters,
    ConversationAdapters,
)
from khoj.database.models import (
    AiModelApi,
    ChatModel,
    PriceTier,
)

call_command(
    "migrate",
    interactive=False,
    verbosity=0,
)

provider, _ = AiModelApi.objects.update_or_create(
    name={LOCAL_CHAT_PROVIDER_NAME!r},
    defaults={{
        "api_key": {LOCAL_CHAT_API_KEY!r},
        "api_base_url": {LOCAL_CHAT_BASE_URL!r},
    }},
)

chat_model = (
    ChatModel.objects
    .filter(
        name={LOCAL_CHAT_MODEL_ID!r},
        ai_model_api=provider,
    )
    .first()
)

if chat_model is None:
    chat_model = ChatModel.objects.create(
        name={LOCAL_CHAT_MODEL_ID!r},
        friendly_name={LOCAL_CHAT_MODEL_NAME!r},
        model_type=ChatModel.ModelType.OPENAI,
        price_tier=PriceTier.FREE,
        max_prompt_size=4096,
        subscribed_max_prompt_size=4096,
        tokenizer=None,
        vision_enabled=False,
        ai_model_api=provider,
        description=(
            "Local Qwen3-4B Q4_K_M model served by "
            "the embedded R2H llama.cpp runtime."
        ),
        strengths=(
            "Private local chat, Arabic and English assistance, "
            "offline OpenAI-compatible generation."
        ),
    )
else:
    chat_model.friendly_name = {LOCAL_CHAT_MODEL_NAME!r}
    chat_model.model_type = ChatModel.ModelType.OPENAI
    chat_model.price_tier = PriceTier.FREE
    chat_model.max_prompt_size = 4096
    chat_model.subscribed_max_prompt_size = 4096
    chat_model.tokenizer = None
    chat_model.vision_enabled = False
    chat_model.description = (
        "Local Qwen3-4B Q4_K_M model served by "
        "the embedded R2H llama.cpp runtime."
    )
    chat_model.strengths = (
        "Private local chat, Arabic and English assistance, "
        "offline OpenAI-compatible generation."
    )
    chat_model.save()

# Remove stale duplicate records for the same model identifier while
# preserving the provider-bound canonical record.
(
    ChatModel.objects
    .filter(name={LOCAL_CHAT_MODEL_ID!r})
    .exclude(pk=chat_model.pk)
    .delete()
)

ConversationAdapters.set_default_chat_model(chat_model)

# This method is idempotent: it creates the default agent when absent
# and refreshes its model/personality when already present.
AgentAdapters.create_default_agent()

selected = ConversationAdapters.get_default_chat_model()

if selected is None or selected.pk != chat_model.pk:
    raise RuntimeError(
        "Khoj did not retain the R2H local default chat model."
    )

print(
    "R2H_KHOJ_LOCAL_CHAT_BOOTSTRAP_PASS "
    f"model_id={{chat_model.id}} "
    f"name={{chat_model.name}} "
    f"provider={{provider.name}} "
    f"base_url={{provider.api_base_url}}",
    flush=True,
)
"""

    result = subprocess.run(
        [
            sys.executable,
            "-u",
            "-c",
            bootstrap_source,
        ],
        env=env,
        cwd=str(KHOJ_SOURCE_DIR),
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        check=False,
    )

    output = result.stdout.strip()

    if output:
        print(output, flush=True)

    if result.returncode != 0:
        raise RuntimeError(
            "Failed to bootstrap the embedded Khoj local chat model. "
            f"Exit code: {result.returncode}"
        )


def main() -> int:
    if not KHOJ_SOURCE_DIR.exists():
        raise FileNotFoundError(f"Khoj source directory not found: {KHOJ_SOURCE_DIR}")

    if not KHOJ_EXE.exists():
        raise FileNotFoundError(f"Khoj executable not found: {KHOJ_EXE}")

    ensure_r2h_settings()
    PG_DATA_DIR.mkdir(parents=True, exist_ok=True)

    print(f"Starting embedded PostgreSQL from: {PG_DATA_DIR}", flush=True)
    server = pgserver.get_server(str(PG_DATA_DIR), cleanup_mode="stop")

    try:
        # pgvector must exist in the default database before Khoj migrations run.
        server.psql("CREATE EXTENSION IF NOT EXISTS vector;")

        database_check = server.psql(
            "SELECT 1 FROM pg_database WHERE datname = 'khoj';"
        )
        if "(1 row)" not in database_check:
            server.psql("CREATE DATABASE khoj;")

        uri = server.get_uri()
        parsed = urlparse(uri)

        if not parsed.hostname or not parsed.port:
            raise RuntimeError(
                f"pgserver did not provide a usable TCP URI: {uri}"
            )

        env = build_environment(parsed)

        print(
            "Bootstrapping R2H local Qwen chat model...",
            flush=True,
        )
        bootstrap_local_chat_model(env)

        print(
            "Embedded PostgreSQL ready on "
            f"{env['POSTGRES_HOST']}:{env['POSTGRES_PORT']}",
            flush=True,
        )
        print(
            "Django compatibility settings: "
            f"{env['DJANGO_SETTINGS_MODULE']} (GMT, USE_TZ=True)",
            flush=True,
        )
        print(
            f"Starting Khoj on http://{KHOJ_HOST}:{KHOJ_PORT}",
            flush=True,
        )

        # Run Khoj's Python entry point directly instead of the Windows
        # khoj.exe console launcher. The launcher can stall when inherited
        # from Tauri's hidden child-process environment.
        command = [
            sys.executable,
            "-u",
            "-c",
            "from khoj.main import run; run()",
            "--host",
            KHOJ_HOST,
            "--port",
            KHOJ_PORT,
            "--anonymous-mode",
        ]

        # The wrapper's stdin is a private control pipe owned by Tauri.
        # Khoj must not inherit it, otherwise its console/runtime startup can
        # block when launched without an interactive Windows console.
        process = subprocess.Popen(
            command,
            env=env,
            cwd=str(KHOJ_SOURCE_DIR),
            stdin=subprocess.DEVNULL,
        )

        stop_requested = threading.Event()

        def watch_parent_commands() -> None:
            for line in sys.stdin:
                if line.strip().upper() == "STOP":
                    stop_requested.set()
                    return
            # The owning R2H process closed the control pipe.
            stop_requested.set()

        watcher = threading.Thread(
            target=watch_parent_commands,
            name="r2h-khoj-parent-control",
            daemon=True,
        )
        watcher.start()

        try:
            while process.poll() is None:
                if stop_requested.wait(timeout=0.25):
                    print("R2H requested Khoj shutdown...", flush=True)
                    process.terminate()
                    try:
                        return process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        return process.wait(timeout=5)
        except KeyboardInterrupt:
            process.terminate()
            try:
                return process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                return process.wait(timeout=5)

        return process.returncode or 0

    finally:
        print("Stopping embedded PostgreSQL...", flush=True)

        try:
            server.cleanup()
        except Exception as error:
            print(
                f"pgserver cleanup warning: {error}",
                file=sys.stderr,
                flush=True,
            )

        # pgserver launches PostgreSQL through a detached Windows cmd process.
        # Explicitly stop the data directory and wait until PostgreSQL exits,
        # so closing R2H cannot leave orphan database processes.
        pg_ctl = (
            Path(pgserver.__file__).resolve().parent
            / "pginstall"
            / "bin"
            / "pg_ctl.exe"
        )

        if pg_ctl.is_file():
            result = subprocess.run(
                [
                    str(pg_ctl),
                    "-D",
                    str(PG_DATA_DIR),
                    "-w",
                    "-t",
                    "120",
                    "-m",
                    "fast",
                    "stop",
                ],
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                check=False,
            )

            output = result.stdout.strip()
            if output:
                print(output, flush=True)

            if result.returncode not in (0, 3):
                print(
                    "Explicit PostgreSQL shutdown returned "
                    f"exit code {result.returncode}.",
                    file=sys.stderr,
                    flush=True,
                )
        else:
            print(
                f"pg_ctl.exe not found: {pg_ctl}",
                file=sys.stderr,
                flush=True,
            )


if __name__ == "__main__":
    raise SystemExit(main())
