# Upgrading R2H Second Brain

Upgrades use the same installer as a fresh install. Run the new
`r2h-second-brain-setup-<NEW-VERSION>-x64.exe` over the existing installation.

## What an upgrade does

1. Stops only the runtimes owned by this product (llama.cpp servers, the
   packaged reranker Python, the embedded Khoj engine, and the desktop app).
   The executable path of every process is verified before it is stopped.
2. Stages the complete new AI pack and Khoj engine payload beside the live
   ones (nothing running is touched during the copy).
3. Hash-verifies the staged payload (models, runtimes, Python, workers).
4. Activates the staged payload with same-volume directory renames. The
   previous payload is retained until activation and verification succeed.
5. Re-verifies the activated payload. If verification fails, the previous
   payload is restored automatically and the installer reports the failure.

## What survives an upgrade

- The knowledge database, indexes, citations, and audit history
  (`%APPDATA%\ai.r2h.second-brain`).
- Chat/PostgreSQL state under `run-data\`.
- Configuration. Only the machine environment variable
  `R2H_SECOND_BRAIN_PROJECT_ROOT` is (re-)written, with the same value.

Nothing outside `C:\ProgramData\R2H.AI-ELE\staging-*`,
`C:\ProgramData\R2H\r2h-second-brain\staging-*`, and the application directory
is ever deleted by the installer.

## Interrupted upgrades

If an installation is interrupted before activation, the previous AI pack
remains fully intact and usable; the partial staging payload is cleaned up by
the next install attempt. If an installation is interrupted between the two
activation renames, the next install detects the leftover `old-*` directory
and restores the last complete payload before continuing.

## Downgrades

Downgrading to an older version is supported with the same installer
mechanics, but the knowledge database schema is forward-only: if a newer
version migrated the database, an older version may refuse to open it. Verify
release notes before downgrading.

## Silent upgrades

```
r2h-second-brain-setup-<NEW-VERSION>-x64.exe /VERYSILENT /NORESTART
```

Silent upgrades follow exactly the same transactional path.
