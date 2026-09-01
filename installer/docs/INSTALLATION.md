# Installing R2H Second Brain

R2H Second Brain installs fully offline. The installer carries the desktop
application, the local AI pack (generation, embedding, reranker models, the
llama.cpp runtime, and a packaged Python 3.12 reranker runtime), and the
embedded Khoj engine runtime. No internet access, package manager, Python
install, or model download is required on the target machine.

## Requirements

- Windows 10 or 11, x64.
- Administrator rights (per-machine installation).
- At least 30 GB free on the system drive (C:) during installation; about
  18 GB after installation.
- 16 GB RAM. The application starts its AI runtimes sequentially to stay
  within a 16 GB budget.

## What is installed where

| Payload | Path | Persistence |
| --- | --- | --- |
| Desktop application | `C:\Program Files\R2H\r2h-second-brain\` | removed on uninstall |
| Local AI pack (models, runtimes, Python) | `C:\ProgramData\R2H.AI-ELE\` | preserved on uninstall by default |
| Khoj engine + scripts | `C:\ProgramData\R2H\r2h-second-brain\` | engines preserved; `run-data\` holds chat/PostgreSQL state |
| Knowledge data (database, indexes, citations, audit) | `C:\Users\<you>\AppData\Roaming\ai.r2h.second-brain\` | preserved on uninstall by default |

The installer sets one machine environment variable,
`R2H_SECOND_BRAIN_PROJECT_ROOT`, pointing at
`C:\ProgramData\R2H\r2h-second-brain`. No other environment configuration is
required.

## Installing

1. Run `r2h-second-brain-setup-<VERSION>-x64.exe`.
2. Follow the wizard. Shortcuts are created in the Start Menu; a desktop
   shortcut is optional.
3. At the end of the wizard, leave "Launch R2H Second Brain" checked.

## Silent installation

```
r2h-second-brain-setup-<VERSION>-x64.exe /SILENT /NORESTART
r2h-second-brain-setup-<VERSION>-x64.exe /VERYSILENT /NORESTART
```

Silent mode provisions the AI pack exactly like the interactive wizard and
skips only the dialogs. Data is never removed in silent mode.

## Verifying the installation

A structural self-check runs automatically at the end of installation. To
re-run it:

```
powershell -ExecutionPolicy Bypass -File "C:\Program Files\R2H\r2h-second-brain\tools\verify-install.ps1"
```

For a QA verification that exercises the Python reranker stack and the Khoj
runtime (slower, runs no destructive action):

```
powershell -ExecutionPolicy Bypass -File "C:\Program Files\R2H\r2h-second-brain\tools\verify-install.ps1" -Full
```

## Services and network

All local AI services bind to `127.0.0.1` only (generation 42111, embedding
42112, reranker 42113, Khoj 42110). The product has no cloud endpoints and no
telemetry path; it does not require internet access at any time.
