# Sphinx

AI-assisted metadata generation for stock photo/video contributors: ingest local
media, analyze it with vision models, generate title/description/keywords,
embed IPTC/XMP, enrich keywords against stock-site APIs, and upload — all from
a Tauri + React desktop app.

Tracked in Jira project **SPHIN**. Implemented so far:

**SPHIN-1 — Ingestion & data layer**

- SPHIN-11 — file picker + drag-and-drop UI
- SPHIN-12 — folder watch mode for automatic ingestion
- SPHIN-13 — file hashing (BLAKE3) and dedupe against previously processed assets
- SPHIN-14 — SQLite schema for `assets` and `jobs`

**SPHIN-2 — Media analysis (AI vision)**

- SPHIN-15 — cloud vision provider integration: OpenAI (GPT-4o), Google Gemini,
  Anthropic Claude, behind one `VisionProvider` trait
- SPHIN-16 — structured content-description prompt: one shared prompt that
  returns strict JSON (`description`, `subjects`, `scene`, `mood`, `colors`,
  `keywords`, `editorial` flag, `text_content`)
- SPHIN-17 — per-project provider configuration: `projects` +
  `project_settings` tables; each project stores its own provider / key /
  model / base-URL / prompt guidance

## Architecture

- `src/` — React + TypeScript frontend (Vite).
- `src-tauri/` — Tauri v2 desktop shell (Rust). Exposes ingestion as
  `#[tauri::command]`s in `src-tauri/src/lib.rs`.
- `src-tauri/crates/core/` (**`sphinx-core`**) — the engine, as a plain Rust
  library with **no Tauri/GUI dependency**:
  - `hash.rs` — streaming BLAKE3 content hashing.
  - `db.rs` — SQLite schema/migrations (`assets`, `jobs`, `projects`,
    `project_settings`, `analyses`), plus queries. Stepwise migrations:
    v1 = SPHIN-1 tables, v2 = SPHIN-2 tables.
  - `ingest.rs` — ingest a path/batch/directory, deduping by path and by
    content hash before writing a new `assets` row.
  - `watch.rs` — debounced recursive folder watching (`notify` +
    `notify-debouncer-mini`), so a burst of file-system events (e.g. a large
    folder copy) collapses into one ingestion pass.
  - `analysis/` — SPHIN-2. `ImageInput` + a `VisionProvider` trait with
    `openai` / `gemini` / `anthropic` implementations (blocking `reqwest`,
    `native-tls` so no extra build toolchain on Windows); `prompt.rs` holds the
    structured prompt; `config.rs` holds `AnalysisConfig` / `ProviderKind`.
    Request shaping and response parsing are unit-tested offline; no test
    touches the network.
  - `models.rs` — `Asset`, `Job`, `Project`, `AnalysisRecord`, `IngestOutcome`.

  Keeping this GUI-free means it can be built and unit tested on its own
  (`cargo test -p sphinx-core`) without a windowing toolchain installed, and
  it's the layer every later epic (metadata generation, embedding, upload)
  builds on.

Database lives at the OS app-data directory (`sphinx.db`), created on first
run via `tauri::AppHandle::path().app_data_dir()`.

## Getting started (Windows)

1. Install [Node.js](https://nodejs.org) (v18+) and [Rust](https://rustup.rs).
2. Install the Tauri Windows prerequisites: the **MSVC C++ Build Tools**
   (via Visual Studio Installer → "Desktop development with C++") and
   **WebView2** (preinstalled on Windows 10/11; Tauri will prompt if missing).
   See https://tauri.app/start/prerequisites/ for details.
3. From the project root:

   ```
   npm install
   npm run tauri dev
   ```

   This launches the app with hot reload. `npm run tauri build` produces an
   installer.

4. Run the `sphinx-core` unit tests on their own at any time (no windowing
   toolchain or network needed):

   ```
   cd src-tauri/crates/core
   cargo test
   ```

### Configuring AI vision (SPHIN-2)

In the app, open **AI vision → Configure**, pick a provider (OpenAI, Gemini or
Claude), paste an API key, and Save. The key is stored in `sphinx.db` on this
machine (encrypted credential storage is SPHIN-25). Then use **Analyze** on any
JPEG/PNG/WebP/GIF asset to get a structured description + candidate keywords.
Model, API base URL and a project-specific prompt line are all optional
overrides.

## A note on how this was built

This project was scaffolded and implemented from a cloud sandbox (Ubuntu,
no root) linked to this machine's `D:\Dev\Workspaces\Claude Code\Sphinx`
folder. That sandbox has no package-manager root access and a restricted
network allowlist, so:

- **SPHIN-1** was built in a Rust 1.75 Ubuntu sandbox: `sphinx-core` compiled
  and its 9 unit tests passed there; the frontend built and type-checked.
- **SPHIN-2** was written in a sandbox where the local Rust toolchain
  (`rustc 1.67`) is too old and its `cargo` binary is broken, so the new
  `sphinx-core` code (`analysis/`, the schema-v2 migration) and the new Tauri
  command handlers **have not been compiler-verified**. The request shaping and
  JSON parsing are covered by offline unit tests (`cargo test -p sphinx-core`),
  which is the first thing to run on this machine after `rustup update`.
- **The Tauri desktop shell** (`src-tauri`) has never been compiled anywhere.
  Its handlers are written against the Tauri v2 API but treat `npm run tauri
  dev` as the first real build/smoke test, on this machine, with an up-to-date
  `rustup` toolchain and the Windows prerequisites above.

First-build checklist on this machine:

```
rustup update
npm install                       # re-fetch node_modules for win32 (was linux)
cd src-tauri/crates/core && cargo test   # offline; must pass
cd ../../.. && npm run tauri dev         # first real shell build
```

## Roadmap (Jira epics)

| Epic | Summary |
| --- | --- |
| SPHIN-1 | Ingestion & data layer *(implemented)* |
| SPHIN-2 | Media analysis (AI vision) *(implemented, pending on-machine build)* |
| SPHIN-3 | Metadata generation |
| SPHIN-4 | Metadata embedding (IPTC/XMP) |
| SPHIN-5 | Keyword enrichment (Shutterstock/Adobe Stock APIs) |
| SPHIN-6 | Upload & distribution (SFTP) |
| SPHIN-7 | Job orchestration & UI |
| SPHIN-8 | Video pipeline |
| SPHIN-9 | Local AI model support (Ollama) |
| SPHIN-10 | Desktop shell & settings |
