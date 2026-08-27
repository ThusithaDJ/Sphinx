use std::path::Path;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::Result;
use crate::models::{Asset, Job};

pub const SCHEMA_VERSION: i32 = 1;

/// Open (creating if needed) the SQLite database at `path` and run migrations.
pub fn open(path: impl AsRef<Path>) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Open an in-memory database (used by tests).
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS assets (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            path        TEXT NOT NULL UNIQUE,
            hash        TEXT NOT NULL,
            size        INTEGER NOT NULL,
            media_type  TEXT NOT NULL,
            status      TEXT NOT NULL DEFAULT 'ingested',
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_assets_hash ON assets(hash);
        CREATE INDEX IF NOT EXISTS idx_assets_status ON assets(status);

        CREATE TABLE IF NOT EXISTS jobs (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            asset_id    INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
            job_type    TEXT NOT NULL,
            status      TEXT NOT NULL DEFAULT 'pending',
            error       TEXT,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_jobs_asset_id ON jobs(asset_id);
        CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
        "#,
    )?;

    conn.execute(
        "INSERT INTO schema_meta(key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![SCHEMA_VERSION.to_string()],
    )?;

    Ok(())
}

/// Look up an existing asset by content hash (used for dedupe).
pub fn find_asset_by_hash(conn: &Connection, hash: &str) -> Result<Option<Asset>> {
    conn.query_row(
        "SELECT id, path, hash, size, media_type, status, created_at, updated_at
         FROM assets WHERE hash = ?1 LIMIT 1",
        params![hash],
        row_to_asset,
    )
    .optional()
    .map_err(Into::into)
}

/// Look up an existing asset by its exact source path.
pub fn find_asset_by_path(conn: &Connection, path: &str) -> Result<Option<Asset>> {
    conn.query_row(
        "SELECT id, path, hash, size, media_type, status, created_at, updated_at
         FROM assets WHERE path = ?1 LIMIT 1",
        params![path],
        row_to_asset,
    )
    .optional()
    .map_err(Into::into)
}

pub fn insert_asset(
    conn: &Connection,
    path: &str,
    hash: &str,
    size: i64,
    media_type: &str,
) -> Result<Asset> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO assets (path, hash, size, media_type, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 'ingested', ?5, ?5)",
        params![path, hash, size, media_type, now],
    )?;
    let id = conn.last_insert_rowid();
    Ok(Asset {
        id,
        path: path.to_string(),
        hash: hash.to_string(),
        size,
        media_type: media_type.to_string(),
        status: "ingested".to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn list_assets(conn: &Connection, limit: i64, offset: i64) -> Result<Vec<Asset>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, hash, size, media_type, status, created_at, updated_at
         FROM assets ORDER BY id DESC LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], row_to_asset)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn count_assets(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT COUNT(*) FROM assets", [], |r| r.get(0))
        .map_err(Into::into)
}

pub fn insert_job(conn: &Connection, asset_id: i64, job_type: &str) -> Result<Job> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO jobs (asset_id, job_type, status, created_at, updated_at)
         VALUES (?1, ?2, 'pending', ?3, ?3)",
        params![asset_id, job_type, now],
    )?;
    let id = conn.last_insert_rowid();
    Ok(Job {
        id,
        asset_id,
        job_type: job_type.to_string(),
        status: "pending".to_string(),
        error: None,
        created_at: now.clone(),
        updated_at: now,
    })
}

fn row_to_asset(row: &rusqlite::Row) -> rusqlite::Result<Asset> {
    Ok(Asset {
        id: row.get(0)?,
        path: row.get(1)?,
        hash: row.get(2)?,
        size: row.get(3)?,
        media_type: row.get(4)?,
        status: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_is_idempotent_and_creates_tables() {
        let conn = open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let version: String = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION.to_string());
    }

    #[test]
    fn insert_and_find_asset_roundtrip() {
        let conn = open_in_memory().unwrap();
        let a = insert_asset(&conn, "/tmp/a.jpg", "hash123", 42, "image").unwrap();
        assert_eq!(a.id, 1);

        let found = find_asset_by_hash(&conn, "hash123").unwrap().unwrap();
        assert_eq!(found.path, "/tmp/a.jpg");

        let found_by_path = find_asset_by_path(&conn, "/tmp/a.jpg").unwrap().unwrap();
        assert_eq!(found_by_path.hash, "hash123");

        assert_eq!(count_assets(&conn).unwrap(), 1);
    }

    #[test]
    fn jobs_reference_assets() {
        let conn = open_in_memory().unwrap();
        let a = insert_asset(&conn, "/tmp/a.jpg", "hash123", 42, "image").unwrap();
        let job = insert_job(&conn, a.id, "analysis").unwrap();
        assert_eq!(job.asset_id, a.id);
        assert_eq!(job.status, "pending");
    }
}
