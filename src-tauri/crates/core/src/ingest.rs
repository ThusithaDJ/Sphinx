use std::path::Path;

use rusqlite::Connection;

use crate::db;
use crate::error::Result;
use crate::hash::hash_file;
use crate::models::{IngestOutcome, MediaType};

/// Ingest a single filesystem path: hash it, classify it, and either record it
/// as a new asset or report it as a duplicate/skip. Never panics on a bad
/// path -- callers ingesting a batch (drag-drop, folder watch) should keep
/// going past one bad file.
pub fn ingest_path(conn: &Connection, path: impl AsRef<Path>) -> Result<IngestOutcome> {
    let path = path.as_ref();
    let path_str = path.to_string_lossy().to_string();

    if !path.is_file() {
        return Ok(IngestOutcome::Skipped {
            path: path_str,
            reason: "not a regular file".into(),
        });
    }

    let media_type = match path
        .extension()
        .and_then(|e| e.to_str())
        .and_then(MediaType::from_extension)
    {
        Some(mt) => mt,
        None => {
            return Ok(IngestOutcome::Skipped {
                path: path_str,
                reason: "unsupported file extension".into(),
            })
        }
    };

    // Already ingested from this exact path -- treat as a no-op duplicate.
    if let Some(existing) = db::find_asset_by_path(conn, &path_str)? {
        return Ok(IngestOutcome::Duplicate {
            existing,
            path: path_str,
        });
    }

    let hash = hash_file(path)?;

    if let Some(existing) = db::find_asset_by_hash(conn, &hash)? {
        return Ok(IngestOutcome::Duplicate {
            existing,
            path: path_str,
        });
    }

    let size = path.metadata()?.len() as i64;
    let asset = db::insert_asset(conn, &path_str, &hash, size, media_type.as_str())?;

    Ok(IngestOutcome::Ingested { asset })
}

/// Ingest a batch of paths (e.g. from a drag-drop event or a folder scan),
/// collecting per-file outcomes rather than aborting the whole batch on one
/// failure.
pub fn ingest_paths<I, P>(conn: &Connection, paths: I) -> Vec<Result<IngestOutcome>>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    paths.into_iter().map(|p| ingest_path(conn, p)).collect()
}

/// Recursively walk a directory and ingest every supported file found in it.
/// Used both for a one-off "ingest this folder" action and as the initial
/// backfill when folder-watch mode is turned on.
pub fn ingest_directory(conn: &Connection, dir: impl AsRef<Path>) -> Vec<Result<IngestOutcome>> {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| ingest_path(conn, e.path()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::io::Write;

    #[test]
    fn ingests_new_file() {
        let conn = db::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("photo.jpg");
        std::fs::File::create(&p).unwrap().write_all(b"data").unwrap();

        let outcome = ingest_path(&conn, &p).unwrap();
        assert!(matches!(outcome, IngestOutcome::Ingested { .. }));
        assert_eq!(db::count_assets(&conn).unwrap(), 1);
    }

    #[test]
    fn dedupes_identical_content_at_a_different_path() {
        let conn = db::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("original.jpg");
        let p2 = dir.path().join("copy.jpg");
        std::fs::File::create(&p1).unwrap().write_all(b"same bytes").unwrap();
        std::fs::File::create(&p2).unwrap().write_all(b"same bytes").unwrap();

        ingest_path(&conn, &p1).unwrap();
        let second = ingest_path(&conn, &p2).unwrap();

        assert!(matches!(second, IngestOutcome::Duplicate { .. }));
        assert_eq!(db::count_assets(&conn).unwrap(), 1);
    }

    #[test]
    fn skips_unsupported_extensions() {
        let conn = db::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("notes.txt");
        std::fs::File::create(&p).unwrap().write_all(b"data").unwrap();

        let outcome = ingest_path(&conn, &p).unwrap();
        assert!(matches!(outcome, IngestOutcome::Skipped { .. }));
        assert_eq!(db::count_assets(&conn).unwrap(), 0);
    }

    #[test]
    fn ingest_directory_walks_recursively() {
        let conn = db::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::File::create(dir.path().join("a.png"))
            .unwrap()
            .write_all(b"aaa")
            .unwrap();
        std::fs::File::create(dir.path().join("sub/b.mp4"))
            .unwrap()
            .write_all(b"bbb")
            .unwrap();

        let results = ingest_directory(&conn, dir.path());
        assert_eq!(results.len(), 2);
        assert_eq!(db::count_assets(&conn).unwrap(), 2);
    }
}
