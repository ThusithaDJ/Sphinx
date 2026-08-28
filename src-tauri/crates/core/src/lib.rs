//! sphinx-core: the engine behind the Sphinx desktop app.
//!
//! Deliberately has zero Tauri/GUI dependencies so it can be built, unit
//! tested, and reasoned about in isolation (`cargo test -p sphinx-core`)
//! without needing a windowing toolchain installed. The `src-tauri` crate
//! wraps these functions as `#[tauri::command]`s for the frontend.
//!
//! Modules by epic:
//! - [`hash`], [`ingest`], [`watch`], [`db`] — SPHIN-1, ingestion & data layer.
//! - [`analysis`] — SPHIN-2, media analysis via cloud vision models.
//! - [`metadata`] — SPHIN-3, title/description/keyword generation from
//!   analysis output plus per-site limiter profiles.
//! - [`embed`] — SPHIN-4, writing metadata into the file as IPTC/XMP via
//!   exiftool, plus CSV export for manual review/upload.
//! - [`keywords`] — SPHIN-5, keyword enrichment via the Shutterstock and
//!   Adobe Stock APIs (the local lookup cache lives in [`db`]).
//! - [`upload`] — SPHIN-6, SFTP upload to a stock site (credentials live in
//!   the OS credential store via [`secrets`], never in `sphinx.db`).
//! - [`video`], [`transcribe`] — SPHIN-8, keyframe/audio extraction and
//!   transcription feeding [`analysis`]'s video-specific prompting.

pub mod analysis;
pub mod db;
pub mod embed;
pub mod error;
pub mod hash;
pub mod ingest;
pub mod keywords;
pub mod metadata;
pub mod models;
pub mod secrets;
pub mod transcribe;
pub mod upload;
pub mod video;
pub mod watch;

pub use error::{CoreError, Result};
