use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult};

use crate::error::{CoreError, Result};

/// A handle to a running folder watch. Dropping it (or calling `stop`) tears
/// down the underlying OS watch.
pub struct WatchHandle {
    _debouncer: notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>,
}

impl WatchHandle {
    pub fn stop(self) {
        // Explicit no-op consumption; Drop does the real teardown. Named
        // `stop` so callers (Tauri commands) have an obvious verb to call.
    }
}

/// Start watching `dir` (recursively) for new/changed files. `on_paths` is
/// invoked with a batch of changed file paths whenever the debounce window
/// (default 500ms) settles, so a burst of file-system events from e.g. a
/// large folder copy collapses into one ingestion pass instead of one per
/// file event.
pub fn watch_folder<F>(dir: impl AsRef<Path>, on_paths: F) -> Result<WatchHandle>
where
    F: Fn(Vec<PathBuf>) + Send + 'static,
{
    let dir = dir.as_ref().to_path_buf();
    if !dir.is_dir() {
        return Err(CoreError::Watch(format!(
            "{} is not a directory",
            dir.display()
        )));
    }

    let (tx, rx) = channel::<DebounceEventResult>();
    let mut debouncer = new_debouncer(Duration::from_millis(500), tx)
        .map_err(|e| CoreError::Watch(e.to_string()))?;

    debouncer
        .watcher()
        .watch(&dir, RecursiveMode::Recursive)
        .map_err(|e| CoreError::Watch(e.to_string()))?;

    std::thread::spawn(move || {
        for result in rx {
            match result {
                Ok(events) => {
                    let paths: Vec<PathBuf> = events
                        .into_iter()
                        .map(|e| e.path)
                        .filter(|p| p.is_file())
                        .collect();
                    if !paths.is_empty() {
                        on_paths(paths);
                    }
                }
                Err(_) => {
                    // The watch channel closed or the OS watcher errored;
                    // nothing more will come through, so just exit the loop.
                    break;
                }
            }
        }
    });

    Ok(WatchHandle {
        _debouncer: debouncer,
    })
}
