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

pub mod analysis;
pub mod db;
pub mod error;
pub mod hash;
pub mod ingest;
pub mod metadata;
pub mod models;
pub mod watch;

pub use error::{CoreError, Result};
