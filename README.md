# Sphinx

AI-assisted metadata generation for stock photo/video contributors: ingest local
media, analyze it with vision models, generate title/description/keywords,
embed IPTC/XMP, enrich keywords against stock-site APIs, and upload — all from
a Tauri + React desktop app.

Tracked in Jira project **SPHIN**. This repo currently implements **SPHIN-1
(Ingestion & data layer)**:

- SPHIN-11 — file picker + drag-and-drop UI
- SPHIN-12 — folder watch mode for automatic ingestion
- SPHIN-13 — file hashing (BLAKE3) and dedupe against previously processed assets
- SPHIN-14 — SQLite schema for `assets` and `jobs`

## Architecture

- `src/` — React + TypeScript frontend (Vite).
- `src-tauri/` — Tauri v2 desktop shell (Rust). Exposes ingestion as
  `#[tauri::command]`s in `src-tauri/src/lib.rs`.
- `src-tauri/crates/core/` (**`sphinx-core`**) — the actual ingestion & data
  layer, as a plain Rust library with **no Tauri/GUI dependency**:
  - `hash.rs` — streaming BLAKE3 content hashing.
  - `db.rs` — SQLite schema/migrations for `assets` and `jobs`, plus queries.
  - `ingest.rs` — ingest a path/batch/directory, deduping by path and by
    content hash before writing a new `assets` row.
  - `watch.rs` — debounced recursive folder watching (`notify` +
    `notify-debouncer-mini`), so a burst of file-system events (e.g. a large
    folder copy) collapses into one ingestion pass.
  - `models.rs` — `Asset`, `Job`, `IngestOutcome`.

  Keeping this GUI-free means it can be built and unit tested on its own
  (`cargo test -p sphinx-core`) without a windowing toolchain installed, and
  it's the layer every later epic (analysis, metadata generation, embedding,
  upload) will build on.

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

4. Run the ingestion-layer unit tests on their own at any time:

   ```
   cd src-tauri/crates/core
   cargo test
   ```

## A note on how this was built

This project was scaffolded and implemented from a cloud sandbox (Ubuntu,
no root) linked to this machine's `D:\Dev\Workspaces\Claude Code\Sphinx`
folder. That sandbox has no package-manager root access and a restricted
network allowlist, so:

- **`sphinx-core`** (the real SPHIN-1 logic) was fully compiled and its 9
  unit tests run successfully there (`cargo test -p sphinx-core` — all
  passing), using a Rust 1.75 toolchain vendored from Ubuntu's `apt` archive
  (no `rustup`, since `static.rust-lang.org` isn't reachable from that
  sandbox).
- **The frontend** (`npm run build`, `tsc --noEmit`) was fully built and
  type-checked there.
- **The Tauri desktop shell itself** (`src-tauri`, the crate that pulls in
  `webkit2gtk`/GTK on Linux) could **not** be compiled in that sandbox: it
  needs both a newer Rust toolchain (several of Tauri's dependencies require
  Rust's 2024 edition, unavailable on 1.75) and the WebKitGTK/GTK3 system
  libraries, which require root to install via `apt` and weren't available.
  The command handlers in `src-tauri/src/lib.rs` were written carefully
  against the Tauri v2 API but have **not** been compiler-verified — treat
  `npm run tauri dev` as the first real build/smoke test, on this machine,
  with a normal `rustup`-installed toolchain. (Windows Tauri prerequisites
  are much lighter than Linux's — no WebKitGTK needed, just MSVC Build Tools
  and WebView2 — so this should compile cleanly once Rust is installed here.)

## Roadmap (Jira epics)

| Epic | Summary |
| --- | --- |
| SPHIN-1 | Ingestion & data layer *(this repo, so far)* |
| SPHIN-2 | Media analysis (AI vision) |
| SPHIN-3 | Metadata generation |
| SPHIN-4 | Metadata embedding (IPTC/XMP) |
| SPHIN-5 | Keyword enrichment (Shutterstock/Adobe Stock APIs) |
| SPHIN-6 | Upload & distribution (SFTP) |
| SPHIN-7 | Job orchestration & UI |
| SPHIN-8 | Video pipeline |
| SPHIN-9 | Local AI model support (Ollama) |
| SPHIN-10 | Desktop shell & settings |
