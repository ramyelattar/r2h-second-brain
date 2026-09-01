# Uninstalling R2H Second Brain

## Standard uninstall

Use Windows Settings > Apps, or Control Panel > Programs and Features, and
uninstall **R2H Second Brain**.

The standard uninstall:

- stops the application and its local AI runtimes (path-verified; processes
  belonging to other products or users are never touched),
- removes the desktop application from `C:\Program Files\R2H\r2h-second-brain`,
- removes Start Menu / desktop shortcuts,
- **keeps** the local AI models in `C:\ProgramData\R2H.AI-ELE\`,
- **keeps** the Khoj engine and all runtime state in
  `C:\ProgramData\R2H\r2h-second-brain\`,
- **keeps** your knowledge database, indexes, citations, and audit history in
  `C:\Users\<you>\AppData\Roaming\ai.r2h.second-brain\`.

Keeping models and data is the safe default: reinstalling later reuses them.

## Full removal (explicit opt-in)

When the uninstaller asks **"Also remove the R2H local AI models ... and all
knowledge data?"**, answering **Yes** additionally deletes:

- `C:\ProgramData\R2H.AI-ELE\` (AI models and runtimes),
- `C:\ProgramData\R2H\r2h-second-brain\` (Khoj engine, scripts, run state),
- `C:\Users\<you>\AppData\Roaming\ai.r2h.second-brain\` (knowledge database).

This is destructive and cannot be undone. In silent uninstall
(`/SILENT`, `/VERYSILENT`) data is always kept; the explicit question is never
answered automatically.

## Notes

- The legacy **R2H.AI-ELE Local AI Pack** uninstaller entry is retired by this
  installer. That old lifecycle could delete the shared AI pack directory; do
  not reinstall it. The Second Brain installer owns the pack from now on.
- The machine environment variable `R2H_SECOND_BRAIN_PROJECT_ROOT` is removed
  with the application.
