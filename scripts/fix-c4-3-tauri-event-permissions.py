from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(r"E:\Projects\r2h-second-brain")
CAPABILITIES = ROOT / "apps" / "desktop" / "src-tauri" / "capabilities"

REQUIRED = [
    "core:event:allow-listen",
    "core:event:allow-unlisten",
]


if not CAPABILITIES.exists():
    raise RuntimeError(f"Capabilities directory not found: {CAPABILITIES}")


files = sorted(
    path
    for path in CAPABILITIES.iterdir()
    if path.is_file() and path.suffix.lower() in {".json", ".json5"}
)

if not files:
    raise RuntimeError("No JSON capability files were found")


updated = 0

for path in files:
    text = path.read_text(encoding="utf-8-sig")

    try:
        data = json.loads(text)
    except json.JSONDecodeError as error:
        raise RuntimeError(
            f"Unable to parse capability file {path}: {error}"
        ) from error

    permissions = data.get("permissions")

    if not isinstance(permissions, list):
        raise RuntimeError(
            f"Capability does not contain a permissions array: {path}"
        )

    backup = path.with_name(path.name + ".before-c4-3-event-permissions")

    if not backup.exists():
        backup.write_text(text, encoding="utf-8", newline="\n")

    changed = False

    for permission in REQUIRED:
        if permission not in permissions:
            permissions.append(permission)
            changed = True

    if changed:
        path.write_text(
            json.dumps(data, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
            newline="\n",
        )
        updated += 1
        print(f"UPDATED {path}")
    else:
        print(f"ALREADY_CONFIGURED {path}")


# Final verification
for path in files:
    data = json.loads(path.read_text(encoding="utf-8-sig"))
    permissions = data.get("permissions", [])

    missing = [
        permission
        for permission in REQUIRED
        if permission not in permissions
    ]

    if missing:
        raise RuntimeError(
            f"Capability verification failed for {path}: {missing}"
        )


print(f"C4_3_EVENT_PERMISSIONS_FIXED files_updated={updated}")
