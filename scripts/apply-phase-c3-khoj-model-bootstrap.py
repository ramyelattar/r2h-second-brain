from __future__ import annotations

from pathlib import Path


ROOT = Path(r"E:\Projects\r2h-second-brain")
WRAPPER = ROOT / "scripts" / "run-khoj-windows.py"


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


backup = WRAPPER.with_name(
    WRAPPER.name + ".before-phase-c3-model-bootstrap"
)

if not backup.exists():
    backup.write_bytes(WRAPPER.read_bytes())

text = WRAPPER.read_text(encoding="utf-8")


# ============================================================
# Add deterministic Khoj model bootstrap source.
# ============================================================

constants_marker = '''KHOJ_PORT = "42110"


SETTINGS_SOURCE ='''

constants_replacement = '''KHOJ_PORT = "42110"

LOCAL_CHAT_MODEL_ID = "qwen3-4b-r2h"
LOCAL_CHAT_MODEL_NAME = "Qwen3-4B"
LOCAL_CHAT_PROVIDER_NAME = "R2H Local llama.cpp"
LOCAL_CHAT_BASE_URL = "http://127.0.0.1:42111/v1/"
LOCAL_CHAT_API_KEY = "r2h-local"


SETTINGS_SOURCE ='''

text = replace_once(
    text,
    constants_marker,
    constants_replacement,
    "local chat constants",
)


# ============================================================
# Add bootstrap function before main().
# ============================================================

main_marker = '''def main() -> int:
'''

bootstrap_function = r'''def bootstrap_local_chat_model(
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
'''

text = replace_once(
    text,
    main_marker,
    bootstrap_function,
    "bootstrap function",
)


# ============================================================
# Call bootstrap after environment creation and before Khoj.
# ============================================================

environment_marker = '''        env = build_environment(parsed)

        print(
            "Embedded PostgreSQL ready on "'''

environment_replacement = '''        env = build_environment(parsed)

        print(
            "Bootstrapping R2H local Qwen chat model...",
            flush=True,
        )
        bootstrap_local_chat_model(env)

        print(
            "Embedded PostgreSQL ready on "'''

text = replace_once(
    text,
    environment_marker,
    environment_replacement,
    "bootstrap invocation",
)

WRAPPER.write_text(
    text,
    encoding="utf-8",
    newline="\n",
)

print(f"UPDATED {WRAPPER}")
print("PHASE_C3_KHOJ_MODEL_BOOTSTRAP_WRITTEN")
