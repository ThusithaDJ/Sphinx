use std::path::Path;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

use crate::analysis::AnalysisConfig;
use crate::error::Result;
use crate::metadata::{GeneratedMetadata, LimiterProfile};
use crate::models::{AnalysisRecord, Asset, Job, MetadataRecord, Project};

pub const SCHEMA_VERSION: i32 = 3;

/// Key under which the per-project analysis config JSON is stored in
/// `project_settings`.
const ANALYSIS_CONFIG_KEY: &str = "analysis_config";

/// Key under which the per-project limiter profile JSON is stored in
/// `project_settings` (SPHIN-19).
const LIMITER_PROFILE_KEY: &str = "limiter_profile";

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

/// Run every migration newer than the database's recorded `schema_version`.
/// Each step is written to be safe to re-run (so an already-migrated DB and a
/// fresh one both end up in the same place).
fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )?;

    let current: i32 = conn
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if current < 1 {
        migrate_v1(conn)?;
    }
    if current < 2 {
        migrate_v2(conn)?;
    }
    if current < 3 {
        migrate_v3(conn)?;
    }

    conn.execute(
        "INSERT INTO schema_meta(key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![SCHEMA_VERSION.to_string()],
    )?;

    Ok(())
}

/// v1 (SPHIN-1): the ingestion & data layer.
fn migrate_v1(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
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
    Ok(())
}

/// v2 (SPHIN-2): projects, per-project settings, and stored analysis results.
fn migrate_v2(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS projects (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            name        TEXT NOT NULL UNIQUE,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS project_settings (
            project_id  INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            key         TEXT NOT NULL,
            value       TEXT NOT NULL,
            PRIMARY KEY (project_id, key)
        );

        CREATE TABLE IF NOT EXISTS analyses (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            asset_id    INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
            provider    TEXT NOT NULL,
            model       TEXT NOT NULL,
            result_json TEXT NOT NULL,
            created_at  TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_analyses_asset_id ON analyses(asset_id);
        "#,
    )?;

    // Associate assets with a project (nullable; ingestion doesn't set it yet —
    // asset→project wiring lands with job orchestration, SPHIN-7).
    add_column_if_missing(
        conn,
        "assets",
        "project_id",
        "INTEGER REFERENCES projects(id)",
    )?;

    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO projects (id, name, created_at, updated_at)
         VALUES (1, 'Default', ?1, ?1)
         ON CONFLICT(id) DO NOTHING",
        params![now],
    )?;

    Ok(())
}

/// v3 (SPHIN-3): generated title/description/keywords per asset.
fn migrate_v3(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS metadata (
            id                     INTEGER PRIMARY KEY AUTOINCREMENT,
            asset_id               INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
            title                  TEXT NOT NULL,
            description            TEXT NOT NULL,
            keywords_json          TEXT NOT NULL,
            profile                TEXT NOT NULL,
            meets_minimum_keywords INTEGER NOT NULL,
            created_at             TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_metadata_asset_id ON metadata(asset_id);
        "#,
    )?;
    Ok(())
}

/// SQLite has no `ADD COLUMN IF NOT EXISTS`; emulate it via `pragma_table_info`.
fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    decl: &str,
) -> Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let existing: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<rusqlite::Result<_>>()?;
    if !existing.iter().any(|c| c == column) {
        conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {decl}"))?;
    }
    Ok(())
}

// --- assets (SPHIN-1) --------------------------------------------------------

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

pub fn get_asset(conn: &Connection, id: i64) -> Result<Option<Asset>> {
    conn.query_row(
        "SELECT id, path, hash, size, media_type, status, created_at, updated_at
         FROM assets WHERE id = ?1",
        params![id],
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

/// Move an asset to a new lifecycle `status` (`ingested` -> `analyzed` -> ...).
pub fn set_asset_status(conn: &Connection, asset_id: i64, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE assets SET status = ?2, updated_at = ?3 WHERE id = ?1",
        params![asset_id, status, Utc::now().to_rfc3339()],
    )?;
    Ok(())
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

// --- jobs (SPHIN-1) --------------------------------------------------------

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

/// Update a job's status, and its `error` column when it failed.
pub fn set_job_status(
    conn: &Connection,
    job_id: i64,
    status: &str,
    error: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE jobs SET status = ?2, error = ?3, updated_at = ?4 WHERE id = ?1",
        params![job_id, status, error, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

// --- projects & settings (SPHIN-2 / SPHIN-17) ------------------------------

pub fn list_projects(conn: &Connection) -> Result<Vec<Project>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, created_at, updated_at FROM projects ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([], row_to_project)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn get_project(conn: &Connection, id: i64) -> Result<Option<Project>> {
    conn.query_row(
        "SELECT id, name, created_at, updated_at FROM projects WHERE id = ?1",
        params![id],
        row_to_project,
    )
    .optional()
    .map_err(Into::into)
}

pub fn create_project(conn: &Connection, name: &str) -> Result<Project> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO projects (name, created_at, updated_at) VALUES (?1, ?2, ?2)",
        params![name, now],
    )?;
    let id = conn.last_insert_rowid();
    Ok(Project {
        id,
        name: name.to_string(),
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn get_project_setting(
    conn: &Connection,
    project_id: i64,
    key: &str,
) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM project_settings WHERE project_id = ?1 AND key = ?2",
        params![project_id, key],
        |r| r.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub fn set_project_setting(
    conn: &Connection,
    project_id: i64,
    key: &str,
    value: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO project_settings (project_id, key, value) VALUES (?1, ?2, ?3)
         ON CONFLICT(project_id, key) DO UPDATE SET value = excluded.value",
        params![project_id, key, value],
    )?;
    Ok(())
}

/// The per-project vision-provider configuration (SPHIN-17), or `None` if the
/// project has never had one saved.
pub fn get_analysis_config(
    conn: &Connection,
    project_id: i64,
) -> Result<Option<AnalysisConfig>> {
    match get_project_setting(conn, project_id, ANALYSIS_CONFIG_KEY)? {
        Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}

pub fn set_analysis_config(
    conn: &Connection,
    project_id: i64,
    config: &AnalysisConfig,
) -> Result<()> {
    let json = serde_json::to_string(config)?;
    set_project_setting(conn, project_id, ANALYSIS_CONFIG_KEY, &json)
}

// --- analyses (SPHIN-2) --------------------------------------------------------

pub fn insert_analysis(
    conn: &Connection,
    asset_id: i64,
    provider: &str,
    model: &str,
    result_json: &str,
) -> Result<AnalysisRecord> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO analyses (asset_id, provider, model, result_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![asset_id, provider, model, result_json, now],
    )?;
    Ok(AnalysisRecord {
        id: conn.last_insert_rowid(),
        asset_id,
        provider: provider.to_string(),
        model: model.to_string(),
        result_json: result_json.to_string(),
        created_at: now,
    })
}

/// The most recent stored analysis for an asset, if any.
pub fn latest_analysis_for_asset(
    conn: &Connection,
    asset_id: i64,
) -> Result<Option<AnalysisRecord>> {
    conn.query_row(
        "SELECT id, asset_id, provider, model, result_json, created_at
         FROM analyses WHERE asset_id = ?1 ORDER BY id DESC LIMIT 1",
        params![asset_id],
        row_to_analysis,
    )
    .optional()
    .map_err(Into::into)
}

/// The per-project limiter profile (SPHIN-19), or `None` if the project has
/// never had one saved (callers should fall back to [`LimiterProfile::default`]).
pub fn get_limiter_profile(
    conn: &Connection,
    project_id: i64,
) -> Result<Option<LimiterProfile>> {
    match get_project_setting(conn, project_id, LIMITER_PROFILE_KEY)? {
        Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}

pub fn set_limiter_profile(
    conn: &Connection,
    project_id: i64,
    profile: &LimiterProfile,
) -> Result<()> {
    let json = serde_json::to_string(profile)?;
    set_project_setting(conn, project_id, LIMITER_PROFILE_KEY, &json)
}

// --- metadata (SPHIN-3) ------------------------------------------------------

pub fn insert_metadata(
    conn: &Connection,
    asset_id: i64,
    meta: &GeneratedMetadata,
) -> Result<MetadataRecord> {
    let now = Utc::now().to_rfc3339();
    let keywords_json = serde_json::to_string(&meta.keywords)?;
    conn.execute(
        "INSERT INTO metadata
            (asset_id, title, description, keywords_json, profile, meets_minimum_keywords, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            asset_id,
            meta.title,
            meta.description,
            keywords_json,
            meta.profile,
            meta.meets_minimum_keywords,
            now
        ],
    )?;
    Ok(MetadataRecord {
        id: conn.last_insert_rowid(),
        asset_id,
        title: meta.title.clone(),
        description: meta.description.clone(),
        keywords_json,
        profile: meta.profile.clone(),
        meets_minimum_keywords: meta.meets_minimum_keywords,
        created_at: now,
    })
}

/// The most recent generated metadata for an asset, if any.
pub fn latest_metadata_for_asset(
    conn: &Connection,
    asset_id: i64,
) -> Result<Option<MetadataRecord>> {
    conn.query_row(
        "SELECT id, asset_id, title, description, keywords_json, profile, meets_minimum_keywords, created_at
         FROM metadata WHERE asset_id = ?1 ORDER BY id DESC LIMIT 1",
        params![asset_id],
        row_to_metadata,
    )
    .optional()
    .map_err(Into::into)
}

// --- row mappers ------------------------------------------------------------

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

fn row_to_project(row: &rusqlite::Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

fn row_to_analysis(row: &rusqlite::Row) -> rusqlite::Result<AnalysisRecord> {
    Ok(AnalysisRecord {
        id: row.get(0)?,
        asset_id: row.get(1)?,
        provider: row.get(2)?,
        model: row.get(3)?,
        result_json: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn row_to_metadata(row: &rusqlite::Row) -> rusqlite::Result<MetadataRecord> {
    Ok(MetadataRecord {
        id: row.get(0)?,
        asset_id: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        keywords_json: row.get(4)?,
        profile: row.get(5)?,
        meets_minimum_keywords: row.get(6)?,
        created_at: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::ProviderKind;
    use crate::metadata::LimiterProfile;

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
    fn migrate_upgrades_a_v1_database_in_place() {
        // Simulate a database created by the SPHIN-1 release: v1 tables only,
        // schema_version pinned at 1, with a row already in `assets`.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
            .unwrap();
        migrate_v1(&conn).unwrap();
        conn.execute(
            "INSERT INTO schema_meta(key, value) VALUES ('schema_version', '1')",
            [],
        )
        .unwrap();
        insert_asset(&conn, "/tmp/a.jpg", "h", 1, "image").unwrap();

        migrate(&conn).unwrap();

        assert!(get_project(&conn, 1).unwrap().is_some());
        assert_eq!(count_assets(&conn).unwrap(), 1);
        // project_id column now exists and is NULL for the pre-existing row.
        let pid: Option<i64> = conn
            .query_row("SELECT project_id FROM assets WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(pid, None);
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

        set_job_status(&conn, job.id, "failed", Some("boom")).unwrap();
        let (status, error): (String, Option<String>) = conn
            .query_row("SELECT status, error FROM jobs WHERE id = ?1", [job.id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(status, "failed");
        assert_eq!(error.as_deref(), Some("boom"));
    }

    #[test]
    fn default_project_exists() {
        let conn = open_in_memory().unwrap();
        let projects = list_projects(&conn).unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "Default");
    }

    #[test]
    fn analysis_config_roundtrips_per_project() {
        let conn = open_in_memory().unwrap();
        assert!(get_analysis_config(&conn, 1).unwrap().is_none());

        let mut cfg = AnalysisConfig::new(ProviderKind::Anthropic);
        cfg.api_key = "secret".into();
        cfg.model = "claude-x".into();
        set_analysis_config(&conn, 1, &cfg).unwrap();

        let back = get_analysis_config(&conn, 1).unwrap().unwrap();
        assert_eq!(back.provider, ProviderKind::Anthropic);
        assert_eq!(back.model, "claude-x");
        assert_eq!(back.api_key, "secret");

        // A second project keeps its own config.
        let p2 = create_project(&conn, "Nature").unwrap();
        assert!(get_analysis_config(&conn, p2.id).unwrap().is_none());
    }

    #[test]
    fn analysis_records_are_stored_newest_first() {
        let conn = open_in_memory().unwrap();
        let a = insert_asset(&conn, "/tmp/a.jpg", "h", 1, "image").unwrap();
        insert_analysis(&conn, a.id, "openai", "gpt-4o", r#"{"v":1}"#).unwrap();
        insert_analysis(&conn, a.id, "openai", "gpt-4o", r#"{"v":2}"#).unwrap();

        let latest = latest_analysis_for_asset(&conn, a.id).unwrap().unwrap();
        assert_eq!(latest.result_json, r#"{"v":2}"#);
    }

    #[test]
    fn limiter_profile_roundtrips_per_project() {
        let conn = open_in_memory().unwrap();
        assert!(get_limiter_profile(&conn, 1).unwrap().is_none());

        let profile = LimiterProfile::adobe_stock();
        set_limiter_profile(&conn, 1, &profile).unwrap();

        let back = get_limiter_profile(&conn, 1).unwrap().unwrap();
        assert_eq!(back, profile);

        let p2 = create_project(&conn, "Nature").unwrap();
        assert!(get_limiter_profile(&conn, p2.id).unwrap().is_none());
    }

    #[test]
    fn metadata_records_are_stored_newest_first() {
        use crate::metadata::GeneratedMetadata;

        let conn = open_in_memory().unwrap();
        let a = insert_asset(&conn, "/tmp/a.jpg", "h", 1, "image").unwrap();

        let first = GeneratedMetadata {
            title: "First".into(),
            description: "First desc".into(),
            keywords: vec!["a".into()],
            profile: "Shutterstock".into(),
            meets_minimum_keywords: false,
        };
        let second = GeneratedMetadata {
            title: "Second".into(),
            description: "Second desc".into(),
            keywords: vec!["a".into(), "b".into()],
            profile: "Shutterstock".into(),
            meets_minimum_keywords: true,
        };
        insert_metadata(&conn, a.id, &first).unwrap();
        insert_metadata(&conn, a.id, &second).unwrap();

        let latest = latest_metadata_for_asset(&conn, a.id).unwrap().unwrap();
        assert_eq!(latest.title, "Second");
        let keywords: Vec<String> = serde_json::from_str(&latest.keywords_json).unwrap();
        assert_eq!(keywords, vec!["a", "b"]);
        assert!(latest.meets_minimum_keywords);
    }
}
