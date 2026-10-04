# Sphinx

**AI-assisted metadata for stock photo and video contributors.**

Sphinx is a free desktop app for Windows, macOS and Linux. Drop in your photos
and videos, and it writes titles, descriptions and keywords for you, shaped to
each stock site's rules. It embeds them into your files and uploads them to
your contributor accounts.

## Features

- **AI analysis**: describes each image or video with a vision model of your
  choice. Cloud options are OpenAI, Google Gemini and Anthropic Claude; a local
  [Ollama](https://ollama.com) model keeps everything on your machine.
- **Per-site metadata**: titles, descriptions and keyword counts are trimmed
  to each site's limits (Adobe Stock, Shutterstock and others, plus custom
  site profiles). Edits can be saved separately for each site.
- **Keyword editor**: keywords are scored and colour-coded by strength, and
  can be reordered, removed, restored or enriched with real search-demand data
  from Shutterstock and Adobe Stock.
- **Video support**: extracts keyframes for analysis, with optional speech
  transcription.
- **Embedding and export**: writes IPTC/XMP metadata into your files with
  exiftool, or exports each site's CSV upload format.
- **Upload**: delivers files over SFTP or FTPS straight to your contributor
  accounts.
- **Batch jobs**: queue analysis, embedding and uploads for many files and
  track them on the Activity screen.

## Install

Download the latest installer from the
[**Releases**](https://github.com/ThusithaDJ/Sphinx/releases) page:

| OS | File |
| --- | --- |
| Windows 10/11 | `-setup.exe` or `.msi` installer, or `_portable.exe` to run without installing |
| macOS (Apple Silicon / Intel) | `.dmg` for your chip (`aarch64` / `x64`) |
| Linux | `.AppImage`, `.deb` or `.rpm` |

**No-install options:** the Windows `_portable.exe` and the Linux `.AppImage`
are single files you can run directly. On Linux, run `chmod +x` on the
AppImage first. The portable exe needs Microsoft's WebView2 runtime, which
Windows 11 already includes; on older Windows 10, use the installer, which
sets it up. Either way, Sphinx stores its library in the data folder listed
below, not next to the program.

> **First launch warning:** the installers aren't signed with a publisher
> certificate yet, so your OS will warn you the first time:
> - **Windows:** if SmartScreen says "Windows protected your PC", click
>   **More info → Run anyway**.
> - **macOS:** drag Sphinx to Applications and open it. When macOS says it
>   can't verify the app, click **Done**, then go to **System Settings →
>   Privacy & Security**, scroll down and click **Open Anyway** next to the
>   Sphinx message. Confirm with your password, then click **Open Anyway**
>   again. You only need to do this once.
>
>   If macOS instead says the app "is damaged", run this in Terminal, then
>   open the app again:
>   `xattr -dr com.apple.quarantine /Applications/Sphinx.app`

### Also install these two free tools

Sphinx relies on them but doesn't bundle them:

| Tool | Used for | Get it |
| --- | --- | --- |
| **ExifTool** | Writing metadata into your files | https://exiftool.org (macOS: `brew install exiftool`, Linux: `apt install libimage-exiftool-perl`) |
| **FFmpeg** | Reading video keyframes and audio | https://ffmpeg.org (macOS: `brew install ffmpeg`, Linux: `apt install ffmpeg`) |

Put both on your `PATH`, or set their locations under **Connections → Local tools**. The About
screen shows whether Sphinx can find them.

## Quick start

1. **Connections**: choose an AI provider and paste its API key, or point
   Sphinx at a local Ollama server. Optionally add SFTP/FTPS logins for your
   stock sites, and keyword-API keys for enrichment.
2. **Import**: add files or a folder. Sphinx can also watch a folder for new
   files.
3. **Library**: run the pipeline on new items (analyze, then generate
   metadata), then review the results. Open any item in the editor to adjust
   its title, description and keywords for each site.
4. **Embed** the metadata into the files, then **upload** them or **export a
   CSV** for the site.

## Privacy and security

- **No account, server, telemetry or tracking.** Sphinx runs entirely on your
  computer.
- **Passwords and API keys go into your operating system's credential store**
  (Windows Credential Manager, macOS Keychain, or GNOME Keyring / KWallet on
  Linux), never into Sphinx's database.
- **Your media only leaves your machine when you choose a service:**
  - A cloud AI provider receives the images or keyframes it analyses, under
    that provider's own terms. Use Ollama to keep media local.
  - Your SFTP/FTPS servers receive the files you upload.

See [SECURITY.md](SECURITY.md) for details and how to report a vulnerability.

### Where your data lives

The library database (`sphinx.db`) is stored in:

| OS | Location |
| --- | --- |
| Windows | `%APPDATA%\com.thusitha.sphinx\` |
| macOS | `~/Library/Application Support/com.thusitha.sphinx/` |
| Linux | `~/.local/share/com.thusitha.sphinx/` |

Deleting that folder resets Sphinx. Saved credentials are listed in your OS
credential store under `sphinx-sftp` and `sphinx-api-keys`. Sphinx never
modifies or deletes your original files, except to write metadata into them
when you embed.

## Building from source

Requirements:

- [Node.js](https://nodejs.org) 18+
- [Rust](https://rustup.rs) (stable)
- The Tauri prerequisites for your OS: https://tauri.app/start/prerequisites/
  - On Linux, also install `libdbus-1-dev` and `pkg-config` (needed for the
    credential store).

Then:

```
npm install
npm run tauri dev      # run with hot reload
npm run tauri build    # build an installer for your OS
```

Run the engine's unit tests (no network or windowing toolkit needed):

```
cd src-tauri/crates/core
cargo test
```

### Project layout

- `src/`: React + TypeScript frontend (Vite).
  - Screens live in `src/routes/`.
  - Shared UI lives in `src/components/`.
  - App state is in `src/state/AppContext.tsx`.
- `src-tauri/`: the Tauri v2 desktop shell. It exposes the engine to the
  frontend as commands in `src-tauri/src/lib.rs`.
- `src-tauri/crates/core/` (`sphinx-core`): the engine, a plain Rust library
  with no GUI dependency.
  - `db.rs`: SQLite schema and numbered migrations.
  - `ingest.rs` / `watch.rs`: importing, BLAKE3 deduplication and folder
    watching.
  - `analysis/`: vision providers (OpenAI, Gemini, Anthropic, Ollama).
  - `metadata/`: per-site limit profiles.
  - `embed/`: exiftool embedding and CSV export.
  - `keywords/`: Shutterstock and Adobe Stock enrichment.
  - `video/` and `transcribe/`: keyframes and speech-to-text.
  - `upload/`: SFTP (`russh`) and FTPS (`suppaftp`).
  - `secrets.rs`: OS credential-store access.

Releases are built by `.github/workflows/release.yml` when a `v*` tag is
pushed.

## Contributing

Issues and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Support the project

If Sphinx saves you time, you can
[buy me a coffee](https://www.buymeacoffee.com/thusithajaw) ☕

## License

[MIT](LICENSE) © Thusitha Jayasundara
