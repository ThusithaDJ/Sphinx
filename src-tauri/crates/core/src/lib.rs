//! sphinx-core: the ingestion & data layer (SPHIN-1).
//!
//! Deliberately has zero Tauri/GUI dependencies so it can be built, unit
//! tested, and reasoned about in isolation (`cargo test -p sphinx-core`)
//! without needing a windowing toolchain installed. The `src-tauri` crate
//! wraps these functions as `#[tauri::command]`s for the frontend.

pub mod db;
pub mod error;
pub mod hash;
pub mod ingest;
pub mod models;
pub mod watch;

pub use error::{CoreError, Result};
