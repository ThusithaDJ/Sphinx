# Sphinx

AI-assisted metadata generation for stock photo/video contributors: ingest local
media, analyze it with vision models (cloud or local), generate
title/description/keywords, embed IPTC/XMP, enrich keywords against stock-site
APIs, and upload — all from a Tauri + React desktop app.

Tracked in Jira project **SPHIN**. All ten epics (SPHIN-1 through SPHIN-10)
are implemented:

| Epic | Summary |
| --- | --- |
| SPHIN-1 | Ingestion & data layer — file picker/drag-drop, folder watch, BLAKE3 hashing/dedupe, SQLite schema |
| SPHIN-2 | Media analysis (AI vision) — OpenAI, Gemini, Anthropic providers behind one `VisionProvider` trait |
| SPHIN-3 | Metadata generation — title/description/keywords + limiter profiles per stock site |
| SPHIN-4 | Metadata embedding — IPTC/XMP via `exiftool`, CSV export |
| SPHIN-5 | Keyword enrichment — Shutterstock + Adobe Stock connectors with caching |
| SPHIN-6 | Upload & distribution — SFTP (pure-Rust `russh`), connection profiles, credential store, retry/backoff |
| SPHIN-7 | Job orchestration & UI — async batch queue with progress and retry |
| SPHIN-8 | Video pipeline — keyframe extraction, optional Whisper transcription, video-aware vision prompting |
| SPHIN-9 | Local AI model support — Ollama vision provider + GPU check |
| SPHIN-10 | Desktop shell & settings — packaging/build pipeline for desktop distribution |

The frontend has since been redesigned into a 7-screen workflow (Import →
Queue → Review → Library → Asset Editor → Sites → Settings), with import
progress, larger library previews, asset delete, custom site profiles, toast
notifications, and working image/video preview in Review and Queue.

## Architecture

- `src/` — React + TypeScript frontend (Vite). Screens live in `src/routes/`
  (`ImportScreen`, `QueueScreen`, `ReviewScreen`, `LibraryScreen`,
  `AssetEditorScreen`, `SitesScreen`, `SettingsScreen`); shared UI in
  `src/components/` (nav bar, stage dots, status strip, keyword chips,
  compliance table, toasts, toggles).
- `src-tauri/` — Tauri v2 desktop shell (Rust). Exposes the engine as
  `#[tauri::command]`s in `src-tauri/src/lib.rs`.
- `src-tauri/crates/core/` (**`sphinx-core`**) — the engine, as a plain Rust
  library with **no Tauri/GUI dependency**, so it can be built and unit
  tested on its own (`cargo test -p sphinx-core`):
  - `hash.rs` — streaming BLAKE3 content hashing.
  - `db.rs` — SQLite schema/migrations (`assets`, `jobs`, `projects`,
    `project_settings`, `analyses`, and later tables added by stepwise
    migrations for metadata, enrichment, upload and video state).
  - `ingest.rs` / `watch.rs` — ingest a path/batch/directory, deduping by
    path and content hash; debounced recursive folder watching (`notify` +
    `notify-debouncer-mini`) so a burst of filesystem events collapses into
    one ingestion pass.
  - `analysis/` — vision providers (`openai.rs`, `gemini.rs`,
    `anthropic.rs`, `ollama.rs`) behind a shared `VisionProvider` trait, a
    structured JSON prompt (`prompt.rs`), and `config.rs` for
    `AnalysisConfig`/`ProviderKind`. Request shaping and response parsing are
    unit-tested offline; no test touches the network.
  - `metadata/` — limiter profiles that shape generated metadata to each
    stock site's rules.
  - `embed/` — IPTC/XMP embedding via `exiftool`, plus CSV export.
  - `keywords/` — Shutterstock and Adobe Stock keyword-enrichment
    connectors with an HTTP client and cache.
  - `upload/` — SFTP upload (`russh`/`russh-sftp`), connection profiles, and
    credential storage (`keyring`) with retry/backoff.
  - `video/` — keyframe extraction and video-aware analysis input; pairs
    with `transcribe/` for optional Whisper transcription.
  - `gpu.rs` — local GPU capability check, used to decide whether the Ollama
    provider is viable on this machine.
  - `secrets.rs` — encrypted credential storage.
  - `models.rs` — shared domain types (`Asset`, `Job`, `Project`,
    `AnalysisRecord`, `IngestOutcome`, etc).

Database lives at the OS app-data directory (`sphinx.db`), created on first
run via `tauri::AppHandle::path().app_data_dir()`.

## Getting started (Windows)

1. Install [Node.js](https://nodejs.org) (v18+) and [Rust](https://rustup.rs).
2. Install the Tauri Windows prerequisites: the **MSVC C++ Build Tools**
   (via Visual Studio Installer → "Desktop development with C++") and
   **WebView2** (preinstalled on Windows 10/11; Tauri will prompt if missing).
   See https://tauri.app/start/prerequisites/ for details.
3. Install [`ffmpeg`](https://ffmpeg.org) and [`exiftool`](https://exiftool.org)
   and make sure both are on `PATH` — they're used for video keyframe
   extraction and IPTC/XMP embedding respectively.
4. From the project root:

   ```
   npm install
   npm run tauri dev
   ```

   This launches the app with hot reload. `npm run tauri build` produces an
   installer (SPHIN-39 packaging pipeline).

5. Run the `sphinx-core` unit tests on their own at any time (no windowing
   toolchain or network needed):

   ```
   cd src-tauri/crates/core
   cargo test
   ```

### Configuring AI vision

In the app, open **Settings → AI vision**, pick a provider (OpenAI, Gemini,
Claude, or a local Ollama model), paste an API key if needed, and Save. Cloud
keys are stored encrypted in `sphinx.db` on this machine. Then run
**Analyze** on an image or video asset to get a structured description +
candidate keywords. Model, API base URL, and a project-specific prompt line
are all optional overrides. For Ollama, Sphinx checks local GPU capability
first and will warn if the machine looks too weak to run vision models
locally.

### Site profiles & upload

**Sites** lets you define custom stock-site profiles (metadata limiter rules)
and SFTP connection profiles for upload. Credentials are stored via the OS
credential store (`keyring`), never in plain text.

## Where things stand

This is implemented on the `ui-redesign` branch (not `main`) — the new
7-screen UI, video pipeline, Ollama support, and packaging work all landed
there. See `new_ui_design/REDESIGN-BRIEF.md` for the design rationale behind
the current screens.
