# R2H Second Brain

**R2H Second Brain** is a local-first desktop knowledge workstation for organizing source material, searching private knowledge, retrieving citation-backed evidence, and interacting with local AI models.

The application combines a **React desktop interface** with a **Tauri/Rust backend** and a modular knowledge-engine architecture.

## Core Capabilities

- Multiple knowledge workspaces
- Local source ingestion
- Search across ingested knowledge
- Citation inspection
- AI chat
- Retrieval-Augmented Generation (RAG)
- Evidence-aware responses
- Local model runtime management
- Khoj runtime integration
- Source inspection
- Imports
- Backup and restore
- Integrity verification
- Audit logs
- Application settings

## Desktop Navigation

The current application exposes dedicated areas for:

```text
Dashboard
Workspaces
Search
Citations
AI Chat
Local Models
Imports
Backup & Restore
Integrity Check
Audit Logs
Settings
```

## Knowledge Workspaces

Information is separated into independent workspaces.

Each workspace can maintain its own:

- Sources
- Search context
- Retrieval results
- Citations
- AI-chat context
- Integrity state
- Audit events

This keeps unrelated knowledge collections isolated at the application level.

## Search & Citations

R2H Second Brain is designed to preserve the connection between an answer and the source material that supports it.

The desktop client models:

- Search hits
- Source records
- Source spans
- Citations
- Retrieval evidence
- RAG retrieval results

Source spans can point to line ranges or page/character ranges, allowing the UI to present evidence at a more precise level than a simple filename reference.

## AI Chat & RAG

The AI-chat layer supports knowledge-aware conversations and retrieval evidence.

The architecture distinguishes between:

- Chat history
- Knowledge modes
- Retrieved evidence
- Citations
- Local model runtime state
- Streaming chat events

This allows the application to expose AI output alongside the local knowledge used to produce it.

## Local Model Runtime

The application includes runtime management for a local model service.

Current UI/runtime contracts support:

- Start
- Stop
- Restart
- Runtime status
- Model selection/path
- Local endpoint status
- Error reporting

The current default runtime model identifier in the desktop client is:

```text
qwen3-4b-r2h
```

Large model files are not intended to be committed to source control.

## Khoj Integration

The desktop application also includes runtime lifecycle management for a local Khoj service.

This allows the application to manage a dedicated local knowledge runtime while retaining R2H-owned UI, workspace, integrity, audit, and local-model orchestration.

## Integrity & Audit

R2H Second Brain includes dedicated application surfaces for:

- Knowledge integrity verification
- Audit-event inspection
- Backup and restore

These features are intended to make changes to local knowledge collections inspectable and recoverable.

## Architecture

The Rust workspace defines dedicated knowledge layers:

```text
knowledge-domain
knowledge-app
knowledge-db
knowledge-storage
knowledge-ingestion
knowledge-search
knowledge-e2e
```

The desktop shell is built separately under:

```text
apps/desktop
```

### Frontend

- React 19
- TypeScript
- Vite
- Tauri API
- Tauri Dialog plugin

### Native / Backend

- Rust
- Tauri 2
- Tokio
- Reqwest
- Serde
- Modular knowledge crates

## Tech Stack

| Layer | Technology |
| --- | --- |
| Desktop Runtime | Tauri 2 |
| Native Backend | Rust |
| Frontend | React 19 |
| Language | TypeScript |
| Bundler | Vite |
| Async Runtime | Tokio |
| HTTP | Reqwest + Rustls |
| Serialization | Serde / JSON |
| Testing | Vitest + Testing Library |
| Package Manager | pnpm |
| Rust Edition | 2024 |

## Repository Layout

```text
.
├── apps/
│   └── desktop/
│       ├── src/
│       └── src-tauri/
├── crates/
│   ├── knowledge-domain/
│   ├── knowledge-app/
│   ├── knowledge-db/
│   ├── knowledge-storage/
│   ├── knowledge-ingestion/
│   ├── knowledge-search/
│   └── knowledge-e2e/
├── installer/
├── scripts/
├── Cargo.toml
├── package.json
└── pnpm-workspace.yaml
```

> The Cargo workspace expects the internal `crates/` modules listed above. A complete native build requires those workspace crates to be present in the local source tree.

## Development

### Requirements

- Node.js
- pnpm
- Rust toolchain
- Tauri system prerequisites

### Install Frontend Dependencies

```bash
pnpm install
```

### Development UI

```bash
pnpm --dir apps/desktop dev
```

### TypeScript / Frontend Build

```bash
pnpm build
```

### Lint

```bash
pnpm lint
```

### Tests

```bash
pnpm test
```

## Code Quality

The Rust workspace applies strict lint rules, including:

- `unsafe_code = "forbid"`
- denied unused results
- denied `unwrap`
- denied `expect`
- denied `panic`
- denied unfinished `todo!`
- denied `unimplemented!`

## Privacy Model

R2H Second Brain is designed around local knowledge and local runtime orchestration.

Knowledge sources, citations, local runtime state, and model files can remain under the user's local control rather than requiring a hosted knowledge service for the core architecture.

## License

**Proprietary.**

The Rust workspace is explicitly marked as non-publishable.

## Repository

[r2h-second-brain](https://github.com/ramyelattar/r2h-second-brain)

---

**R2H — Your knowledge, searchable and usable on your own machine.**
