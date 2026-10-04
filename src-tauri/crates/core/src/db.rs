use std::collections::HashMap;
use std::path::Path;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

use crate::analysis::AnalysisConfig;
use crate::embed::EmbedConfig;
use crate::error::{CoreError, Result};
use crate::keywords::KeywordConfig;
use crate::metadata::{GeneratedMetadata, LimiterProfile};
use crate::models::{AnalysisRecord, Asset, Job, JobEvent, MetadataRecord, Project, SftpProfileRecord};
use crate::transcribe::TranscriptionConfig;
use crate::video::VideoConfig;

pub const SCHEMA_VERSION: i32 = 9;

/// Key under which the per-project analysis config JSON is stored in
/// `project_settings`. This holds the currently *active* provider's config
/// (the one [`analyze_file`](crate::analysis::analyze_file) actually uses).
const ANALYSIS_CONFIG_KEY: &str = "analysis_config";

/// Key under which a project's per-provider analysis configs are stored, as a
/// JSON map of provider name -> [`AnalysisConfig`]. Lets a project keep, say,
/// both an OpenAI and a Gemini config on hand and switch the active one
/// without re-entering the other's API key/model/etc.
const ANALYSIS_CONFIGS_KEY: &str = "analysis_configs";

/// Key under which the per-project limiter profile JSON is stored in
/// `project_settings` (SPHIN-19).
const LIMITER_PROFILE_KEY: &str = "limiter_profile";

/// Key under which a project's user-added custom site profiles are stored,
/// as a JSON array of [`LimiterProfile`] (on top of the fixed built-in
/// presets, which aren't stored -- see [`get_custom_site_profiles`]).
const CUSTOM_SITE_PROFILES_KEY: &str = "custom_site_profiles";

/// Key under which the names of built-in presets the user deleted from a
/// project are stored, as a JSON array of strings.
const HIDDEN_BUILT_IN_SITES_KEY: &str = "hidden_built_in_sites";

/// Key under which the per-project embed config JSON is stored in
/// `project_settings` (SPHIN-20).
const EMBED_CONFIG_KEY: &str = "embed_config";

/// Key under which the per-project keyword-enrichment config JSON is stored
/// in `project_settings` (SPHIN-5).
const KEYWORD_CONFIG_KEY: &str = "keyword_config";

/// Key under which user-edited CSV export column layouts (a JSON object keyed
/// by site name) are stored in `project_settings`. The frontend owns the
/// shape; the backend just persists it.
const CSV_LAYOUTS_KEY: &str = "csv_layouts";

/// Key under which the per-project video (ffmpeg) config JSON is stored in
/// `project_settings` (SPHIN-31).
const VIDEO_CONFIG_KEY: &str = "video_config";

/// Key under which the per-project transcription config JSON is stored in
/// `project_settings` (SPHIN-32).
const TRANSCRIPTION_CONFIG_KEY: &str = "transcription_config";

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
    if current < 4 {
        migrate_v4(conn)?;
    }
    if current < 5 {
        migrate_v5(conn)?;
    }
    if current < 6 {
        migrate_v6(conn)?;
    }
    if current < 7 {
        migrate_v7(conn)?;
    }
    if current < 8 {
        migrate_v8(conn)?;
    }
    if current < 9 {
        migrate_v9(conn)?;
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

/// v4 (SPHIN-5): local cache of per-site keyword suggestions, keyed by
/// normalized seed term + provider, so repeated lookups don't burn through
/// each API's rate limit (SPHIN-24).
fn migrate_v4(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS keyword_cache (
            seed          TEXT NOT NULL,
            provider      TEXT NOT NULL,
            keywords_json TEXT NOT NULL,
            fetched_at    TEXT NOT NULL,
            PRIMARY KEY (seed, provider)
        );
        "#,
    )?;
    Ok(())
}

/// v5 (SPHIN-7): job orchestration. `source` separates the pre-existing
/// direct-command audit log from the new batch queue (see [`crate::models::Job`]);
/// `payload_json` carries per-job context (e.g. which project); `attempts`
/// backs retry (SPHIN-30).
fn migrate_v5(conn: &Connection) -> Result<()> {
    add_column_if_missing(conn, "jobs", "source", "TEXT NOT NULL DEFAULT 'direct'")?;
    add_column_if_missing(conn, "jobs", "payload_json", "TEXT NOT NULL DEFAULT '{}'")?;
    add_column_if_missing(conn, "jobs", "attempts", "INTEGER NOT NULL DEFAULT 0")?;
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_jobs_source_status ON jobs(source, status);",
    )?;
    Ok(())
}

/// v6 (SPHIN-6): SFTP connection profiles, and a `not_before` column on
/// `jobs` for exponential backoff on automatic retry (SPHIN-26).
fn migrate_v6(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS sftp_profiles (
            id                    INTEGER PRIMARY KEY AUTOINCREMENT,
            project_id            INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
            name                  TEXT NOT NULL,
            site                  TEXT NOT NULL DEFAULT 'generic',
            host                  TEXT NOT NULL,
            port                  INTEGER NOT NULL,
            username              TEXT NOT NULL,
            remote_dir            TEXT NOT NULL,
            credential_key        TEXT NOT NULL UNIQUE,
            host_key_fingerprint  TEXT,
            created_at            TEXT NOT NULL,
            updated_at            TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_sftp_profiles_project_id ON sftp_profiles(project_id);
        "#,
    )?;
    add_column_if_missing(conn, "jobs", "not_before", "TEXT")?;
    Ok(())
}

/// v7: per-job event history. `jobs` itself only ever holds the *current*
/// status/error -- every transition function overwrites them -- so this adds
/// an append-only table with one row per transition a job actually goes
/// through, for Activity's per-job log.
fn migrate_v7(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS job_events (
            id       INTEGER PRIMARY KEY AUTOINCREMENT,
            job_id   INTEGER NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
            status   TEXT NOT NULL,
            message  TEXT NOT NULL,
            at       TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_job_events_job_id ON job_events(job_id);
        "#,
    )?;
    Ok(())
}

/// v8: `protocol` column on `sftp_profiles`, so a delivery profile can speak
/// FTPS instead of SFTP -- some stock sites (e.g. Shutterstock) only offer
/// FTPS. Existing profiles default to `'sftp'`, preserving current behavior.
fn migrate_v8(conn: &Connection) -> Result<()> {
    add_column_if_missing(conn, "sftp_profiles", "protocol", "TEXT NOT NULL DEFAULT 'sftp'")?;
    Ok(())
}

/// v9: per-site metadata drafts. The `metadata` table keeps the AI-generated
/// default; each stock site can carry its own edited title/description/
/// keywords on top of it, chosen in the asset editor's "Preview as" switch.
fn migrate_v9(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS site_metadata (
            asset_id      INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
            site_name     TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            updated_at    TEXT NOT NULL,
            PRIMARY KEY (asset_id, site_name)
        );
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

/// Remove an asset from the library. Only drops the catalog row (and, via
/// `ON DELETE CASCADE`, its jobs/analyses/metadata) -- the original file on
/// disk is never touched, since assets are referenced in place rather than
/// owned/copied by Sphinx.
pub fn delete_asset(conn: &Connection, asset_id: i64) -> Result<()> {
    conn.execute("DELETE FROM assets WHERE id = ?1", params![asset_id])?;
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

/// Log a job for a single direct (synchronous, single-asset command)
/// action. Always `source = 'direct'`, so the batch worker (SPHIN-7) never
/// touches it. Starts `pending`; callers immediately follow up with
/// [`set_job_status`].
pub fn insert_job(conn: &Connection, asset_id: i64, job_type: &str) -> Result<Job> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO jobs (asset_id, job_type, status, source, payload_json, attempts, created_at, updated_at)
         VALUES (?1, ?2, 'pending', 'direct', '{}', 0, ?3, ?3)",
        params![asset_id, job_type, now],
    )?;
    let id = conn.last_insert_rowid();
    Ok(Job {
        id,
        asset_id,
        job_type: job_type.to_string(),
        status: "pending".to_string(),
        error: None,
        source: "direct".to_string(),
        payload_json: "{}".to_string(),
        attempts: 0,
        not_before: None,
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
    let changed = conn.execute(
        "UPDATE jobs SET status = ?2, error = ?3, updated_at = ?4 WHERE id = ?1",
        params![job_id, status, error, Utc::now().to_rfc3339()],
    )?;
    if changed > 0 {
        log_job_event(conn, job_id, status, error.unwrap_or("Completed"))?;
    }
    Ok(())
}

/// Record one status-transition event for a job. Append-only, unlike
/// `jobs.status`/`error` themselves (which each new transition overwrites) --
/// this is what preserves the history Activity's per-job log shows.
fn log_job_event(conn: &Connection, job_id: i64, status: &str, message: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO job_events (job_id, status, message, at) VALUES (?1, ?2, ?3, ?4)",
        params![job_id, status, message, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

/// A job's full event history, oldest first, for Activity's per-job log.
pub fn list_job_events(conn: &Connection, job_id: i64) -> Result<Vec<JobEvent>> {
    let mut stmt = conn.prepare(
        "SELECT id, job_id, status, message, at FROM job_events WHERE job_id = ?1 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![job_id], |row| {
        Ok(JobEvent {
            id: row.get(0)?,
            job_id: row.get(1)?,
            status: row.get(2)?,
            message: row.get(3)?,
            at: row.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

// --- job queue (SPHIN-7) -----------------------------------------------------

/// Enqueue one batch job per asset in `asset_ids`, all `source = 'queue'`
/// and `status = 'pending'`, ready for the background worker to claim.
pub fn enqueue_jobs(
    conn: &Connection,
    asset_ids: &[i64],
    job_type: &str,
    payload_json: &str,
) -> Result<Vec<Job>> {
    let now = Utc::now().to_rfc3339();
    let mut jobs = Vec::with_capacity(asset_ids.len());
    for &asset_id in asset_ids {
        conn.execute(
            "INSERT INTO jobs (asset_id, job_type, status, source, payload_json, attempts, created_at, updated_at)
             VALUES (?1, ?2, 'pending', 'queue', ?3, 0, ?4, ?4)",
            params![asset_id, job_type, payload_json, now],
        )?;
        let id = conn.last_insert_rowid();
        log_job_event(conn, id, "pending", "Queued")?;
        jobs.push(Job {
            id,
            asset_id,
            job_type: job_type.to_string(),
            status: "pending".to_string(),
            error: None,
            source: "queue".to_string(),
            payload_json: payload_json.to_string(),
            attempts: 0,
            not_before: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        });
    }
    Ok(jobs)
}

/// Atomically claim the oldest pending, currently-eligible (past its
/// `not_before`, if any) queue job, transitioning it to `running`. Safe
/// under concurrent callers only in that they all serialize through the
/// same [`Connection`] (guarded by a `Mutex` at the app layer); this
/// function does not itself provide cross-connection locking.
pub fn claim_next_pending_job(conn: &Connection) -> Result<Option<Job>> {
    let now = Utc::now().to_rfc3339();
    let job = conn
        .query_row(
            "SELECT id, asset_id, job_type, status, error, source, payload_json, attempts, not_before, created_at, updated_at
             FROM jobs
             WHERE status = 'pending' AND source = 'queue' AND (not_before IS NULL OR not_before <= ?1)
             ORDER BY id ASC LIMIT 1",
            params![now],
            row_to_job,
        )
        .optional()?;
    let Some(job) = job else {
        return Ok(None);
    };
    conn.execute(
        "UPDATE jobs SET status = 'running', updated_at = ?2 WHERE id = ?1",
        params![job.id, Utc::now().to_rfc3339()],
    )?;
    log_job_event(conn, job.id, "running", &format!("Started (attempt {})", job.attempts + 1))?;
    Ok(Some(Job {
        status: "running".to_string(),
        ..job
    }))
}

/// Reset a failed (or stuck) queue job back to `pending` for an immediate
/// manual retry, clearing its error/backoff and bumping `attempts`. SPHIN-30.
pub fn retry_job(conn: &Connection, job_id: i64) -> Result<()> {
    let changed = conn.execute(
        "UPDATE jobs SET status = 'pending', error = NULL, not_before = NULL, attempts = attempts + 1, updated_at = ?2
         WHERE id = ?1 AND source = 'queue'",
        params![job_id, Utc::now().to_rfc3339()],
    )?;
    if changed > 0 {
        log_job_event(conn, job_id, "pending", "Manual retry requested")?;
    }
    Ok(())
}

/// Automatic backoff retry (SPHIN-26): reschedule a failed queue job to
/// become claimable again after `delay_secs`, keeping `error` visible in
/// the meantime so the dashboard shows why it's about to retry.
pub fn schedule_retry(conn: &Connection, job_id: i64, delay_secs: i64, error: &str) -> Result<()> {
    let not_before = (Utc::now() + chrono::Duration::seconds(delay_secs)).to_rfc3339();
    let changed = conn.execute(
        "UPDATE jobs SET status = 'pending', attempts = attempts + 1, not_before = ?2, error = ?3, updated_at = ?4
         WHERE id = ?1",
        params![job_id, not_before, error, Utc::now().to_rfc3339()],
    )?;
    if changed > 0 {
        log_job_event(
            conn,
            job_id,
            "pending",
            &format!("Attempt failed: {error}. Retrying in {delay_secs}s."),
        )?;
    }
    Ok(())
}

/// Cancel a queue job that hasn't started yet, before the worker ever claims
/// it. Returns `false` (a no-op) if the job wasn't `pending` -- either it's
/// already running (the caller falls back to signalling the worker) or it's
/// already in a terminal state.
pub fn cancel_pending_job(conn: &Connection, job_id: i64) -> Result<bool> {
    let changed = conn.execute(
        "UPDATE jobs SET status = 'cancelled', updated_at = ?2 WHERE id = ?1 AND status = 'pending'",
        params![job_id, Utc::now().to_rfc3339()],
    )?;
    if changed > 0 {
        log_job_event(conn, job_id, "cancelled", "Cancelled by user")?;
    }
    Ok(changed > 0)
}

/// Delete finished queue jobs (and, via cascade, their event history). Only
/// jobs in a terminal state (`done`/`failed`/`cancelled`) are removed, so a
/// pending or running job is never pulled out from under the worker.
/// `job_ids = None` removes every queue job with `status`. Returns how many
/// rows were deleted.
pub fn delete_finished_jobs(conn: &Connection, job_ids: Option<&[i64]>, status: Option<&str>) -> Result<usize> {
    const TERMINAL: &str = "status IN ('done', 'failed', 'cancelled') AND source = 'queue'";
    let mut removed = 0;
    match job_ids {
        Some(ids) => {
            for id in ids {
                removed += conn.execute(&format!("DELETE FROM jobs WHERE id = ?1 AND {TERMINAL}"), params![id])?;
            }
        }
        None => {
            removed += conn.execute(
                &format!("DELETE FROM jobs WHERE (?1 IS NULL OR status = ?1) AND {TERMINAL}"),
                params![status],
            )?;
        }
    }
    Ok(removed)
}

/// Queue jobs (batch work only, not the direct-command audit log), most
/// recent first, for the progress dashboard. SPHIN-29.
pub fn list_queue_jobs(conn: &Connection, limit: i64) -> Result<Vec<Job>> {
    let mut stmt = conn.prepare(
        "SELECT id, asset_id, job_type, status, error, source, payload_json, attempts, not_before, created_at, updated_at
         FROM jobs WHERE source = 'queue' ORDER BY id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], row_to_job)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// How many queue jobs are in each status, for a summary badge.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct JobCounts {
    pub pending: i64,
    pub running: i64,
    pub done: i64,
    pub failed: i64,
    pub cancelled: i64,
}

pub fn queue_job_counts(conn: &Connection) -> Result<JobCounts> {
    let mut counts = JobCounts::default();
    let mut stmt =
        conn.prepare("SELECT status, COUNT(*) FROM jobs WHERE source = 'queue' GROUP BY status")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
    for row in rows {
        let (status, count) = row?;
        match status.as_str() {
            "pending" => counts.pending = count,
            "running" => counts.running = count,
            "done" => counts.done = count,
            "failed" => counts.failed = count,
            "cancelled" => counts.cancelled = count,
            _ => {}
        }
    }
    Ok(counts)
}

// --- SFTP profiles (SPHIN-25) -------------------------------------------------

/// Create a connection profile. The password is not part of this call --
/// callers save it separately via [`crate::secrets::set_secret`] under the
/// returned profile's `credential_key`.
#[allow(clippy::too_many_arguments)]
pub fn create_sftp_profile(
    conn: &Connection,
    project_id: i64,
    name: &str,
    site: &str,
    protocol: &str,
    host: &str,
    port: i64,
    username: &str,
    remote_dir: &str,
) -> Result<SftpProfileRecord> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO sftp_profiles
            (project_id, name, site, protocol, host, port, username, remote_dir, credential_key, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, '', ?9, ?9)",
        params![project_id, name, site, protocol, host, port, username, remote_dir, now],
    )?;
    let id = conn.last_insert_rowid();
    // The credential key is derived from the row id, so it can't be chosen
    // until after the insert; a second statement fills it in.
    let credential_key = format!("sftp-profile-{id}");
    conn.execute(
        "UPDATE sftp_profiles SET credential_key = ?2 WHERE id = ?1",
        params![id, credential_key],
    )?;
    Ok(SftpProfileRecord {
        id,
        project_id,
        name: name.to_string(),
        site: site.to_string(),
        protocol: protocol.to_string(),
        host: host.to_string(),
        port,
        username: username.to_string(),
        remote_dir: remote_dir.to_string(),
        credential_key,
        host_key_fingerprint: None,
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn list_sftp_profiles(conn: &Connection, project_id: i64) -> Result<Vec<SftpProfileRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, project_id, name, site, protocol, host, port, username, remote_dir, credential_key, host_key_fingerprint, created_at, updated_at
         FROM sftp_profiles WHERE project_id = ?1 ORDER BY id ASC",
    )?;
    let rows = stmt.query_map(params![project_id], row_to_sftp_profile)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn get_sftp_profile(conn: &Connection, id: i64) -> Result<Option<SftpProfileRecord>> {
    conn.query_row(
        "SELECT id, project_id, name, site, protocol, host, port, username, remote_dir, credential_key, host_key_fingerprint, created_at, updated_at
         FROM sftp_profiles WHERE id = ?1",
        params![id],
        row_to_sftp_profile,
    )
    .optional()
    .map_err(Into::into)
}

/// Update everything about a profile except its `credential_key` and
/// `host_key_fingerprint`, which have their own dedicated setters.
#[allow(clippy::too_many_arguments)]
pub fn update_sftp_profile(
    conn: &Connection,
    id: i64,
    name: &str,
    site: &str,
    protocol: &str,
    host: &str,
    port: i64,
    username: &str,
    remote_dir: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE sftp_profiles SET name = ?2, site = ?3, protocol = ?4, host = ?5, port = ?6, username = ?7, remote_dir = ?8, updated_at = ?9
         WHERE id = ?1",
        params![id, name, site, protocol, host, port, username, remote_dir, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

/// Pin (or update) the host key fingerprint seen on a successful connection
/// (trust-on-first-use, SPHIN-6).
pub fn set_sftp_host_key_fingerprint(conn: &Connection, id: i64, fingerprint: &str) -> Result<()> {
    conn.execute(
        "UPDATE sftp_profiles SET host_key_fingerprint = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, fingerprint, Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

pub fn delete_sftp_profile(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM sftp_profiles WHERE id = ?1", params![id])?;
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
    set_project_setting(conn, project_id, ANALYSIS_CONFIG_KEY, &json)?;

    let mut configs = get_analysis_configs(conn, project_id)?;
    configs.insert(config.provider.as_str().to_string(), config.clone());
    let configs_json = serde_json::to_string(&configs)?;
    set_project_setting(conn, project_id, ANALYSIS_CONFIGS_KEY, &configs_json)
}

/// Every provider config a project has ever saved, keyed by provider name
/// (SPHIN-17 follow-up). The currently-active config is always included even
/// if it predates this map (e.g. data saved before this feature existed),
/// so callers never lose track of what's actually in use.
pub fn get_analysis_configs(
    conn: &Connection,
    project_id: i64,
) -> Result<HashMap<String, AnalysisConfig>> {
    let mut configs: HashMap<String, AnalysisConfig> =
        match get_project_setting(conn, project_id, ANALYSIS_CONFIGS_KEY)? {
            Some(json) => serde_json::from_str(&json)?,
            None => HashMap::new(),
        };
    if let Some(active) = get_analysis_config(conn, project_id)? {
        configs
            .entry(active.provider.as_str().to_string())
            .or_insert(active);
    }
    Ok(configs)
}

/// Replace a project's whole per-provider config map without touching which
/// provider is active (used when rewriting stored configs in place).
pub fn set_analysis_configs(
    conn: &Connection,
    project_id: i64,
    configs: &HashMap<String, AnalysisConfig>,
) -> Result<()> {
    let json = serde_json::to_string(configs)?;
    set_project_setting(conn, project_id, ANALYSIS_CONFIGS_KEY, &json)
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

/// A project's user-added custom site profiles (on top of
/// [`LimiterProfile::built_ins`]), letting a project target sites beyond the
/// fixed preset list.
pub fn get_custom_site_profiles(conn: &Connection, project_id: i64) -> Result<Vec<LimiterProfile>> {
    match get_project_setting(conn, project_id, CUSTOM_SITE_PROFILES_KEY)? {
        Some(json) => Ok(serde_json::from_str(&json)?),
        None => Ok(Vec::new()),
    }
}

fn set_custom_site_profiles(conn: &Connection, project_id: i64, profiles: &[LimiterProfile]) -> Result<()> {
    let json = serde_json::to_string(profiles)?;
    set_project_setting(conn, project_id, CUSTOM_SITE_PROFILES_KEY, &json)
}

/// Add a custom site profile, replacing any existing custom profile with the
/// same name (so re-saving an edit doesn't create a duplicate). Returns the
/// full updated list.
pub fn add_custom_site_profile(
    conn: &Connection,
    project_id: i64,
    profile: LimiterProfile,
) -> Result<Vec<LimiterProfile>> {
    let name = profile.name.trim().to_string();
    if name.is_empty() {
        return Err(CoreError::Validation("site profile name cannot be blank".to_string()));
    }
    if LimiterProfile::built_ins()
        .iter()
        .any(|b| b.name.eq_ignore_ascii_case(&name))
    {
        return Err(CoreError::Validation(format!(
            "\"{name}\" is already a built-in site profile"
        )));
    }
    let mut profiles = get_custom_site_profiles(conn, project_id)?;
    profiles.retain(|p| !p.name.eq_ignore_ascii_case(&name));
    let mut profile = profile;
    profile.name = name;
    profiles.push(profile);
    set_custom_site_profiles(conn, project_id, &profiles)?;
    Ok(profiles)
}

/// Remove a custom site profile by name. Returns the full updated list.
pub fn remove_custom_site_profile(
    conn: &Connection,
    project_id: i64,
    name: &str,
) -> Result<Vec<LimiterProfile>> {
    let mut profiles = get_custom_site_profiles(conn, project_id)?;
    profiles.retain(|p| p.name != name);
    set_custom_site_profiles(conn, project_id, &profiles)?;
    Ok(profiles)
}

fn get_hidden_built_in_sites(conn: &Connection, project_id: i64) -> Result<Vec<String>> {
    match get_project_setting(conn, project_id, HIDDEN_BUILT_IN_SITES_KEY)? {
        Some(json) => Ok(serde_json::from_str(&json)?),
        None => Ok(Vec::new()),
    }
}

fn set_hidden_built_in_sites(conn: &Connection, project_id: i64, names: &[String]) -> Result<()> {
    let json = serde_json::to_string(names)?;
    set_project_setting(conn, project_id, HIDDEN_BUILT_IN_SITES_KEY, &json)
}

fn built_in_named(name: &str) -> Option<LimiterProfile> {
    LimiterProfile::built_ins()
        .into_iter()
        .find(|b| b.name.eq_ignore_ascii_case(name.trim()))
}

/// Every site profile visible in a project: the built-in presets the user
/// hasn't deleted, followed by the project's custom profiles.
pub fn list_site_profiles(conn: &Connection, project_id: i64) -> Result<Vec<LimiterProfile>> {
    let hidden = get_hidden_built_in_sites(conn, project_id)?;
    let mut profiles: Vec<LimiterProfile> = LimiterProfile::built_ins()
        .into_iter()
        .filter(|b| !hidden.iter().any(|h| h.eq_ignore_ascii_case(&b.name)))
        .collect();
    profiles.extend(get_custom_site_profiles(conn, project_id)?);
    Ok(profiles)
}

/// Add a site profile. A name matching a deleted built-in preset restores
/// that preset (with its stock limits); anything else is added as a custom
/// profile. Returns the full visible list.
pub fn add_site_profile(conn: &Connection, project_id: i64, profile: LimiterProfile) -> Result<Vec<LimiterProfile>> {
    if let Some(built_in) = built_in_named(&profile.name) {
        let mut hidden = get_hidden_built_in_sites(conn, project_id)?;
        if !hidden.iter().any(|h| h.eq_ignore_ascii_case(&built_in.name)) {
            return Err(CoreError::Validation(format!(
                "\"{}\" is already a site profile",
                built_in.name
            )));
        }
        hidden.retain(|h| !h.eq_ignore_ascii_case(&built_in.name));
        set_hidden_built_in_sites(conn, project_id, &hidden)?;
    } else {
        add_custom_site_profile(conn, project_id, profile)?;
    }
    list_site_profiles(conn, project_id)
}

/// Delete a site profile: a built-in preset is hidden for this project, a
/// custom one is removed outright. Returns the full visible list.
pub fn remove_site_profile(conn: &Connection, project_id: i64, name: &str) -> Result<Vec<LimiterProfile>> {
    if let Some(built_in) = built_in_named(name) {
        let mut hidden = get_hidden_built_in_sites(conn, project_id)?;
        if !hidden.iter().any(|h| h.eq_ignore_ascii_case(&built_in.name)) {
            hidden.push(built_in.name);
            set_hidden_built_in_sites(conn, project_id, &hidden)?;
        }
    } else {
        remove_custom_site_profile(conn, project_id, name)?;
    }
    list_site_profiles(conn, project_id)
}

/// The per-project embed (exiftool) configuration (SPHIN-20), or `None` if
/// the project has never had one saved (callers should fall back to
/// [`EmbedConfig::default`]).
pub fn get_embed_config(conn: &Connection, project_id: i64) -> Result<Option<EmbedConfig>> {
    match get_project_setting(conn, project_id, EMBED_CONFIG_KEY)? {
        Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}

pub fn set_embed_config(conn: &Connection, project_id: i64, config: &EmbedConfig) -> Result<()> {
    let json = serde_json::to_string(config)?;
    set_project_setting(conn, project_id, EMBED_CONFIG_KEY, &json)
}

/// The per-project keyword-enrichment configuration (SPHIN-5), or `None` if
/// the project has never had one saved (callers should fall back to
/// [`KeywordConfig::default`], which enables no connectors).
pub fn get_keyword_config(conn: &Connection, project_id: i64) -> Result<Option<KeywordConfig>> {
    match get_project_setting(conn, project_id, KEYWORD_CONFIG_KEY)? {
        Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}

pub fn set_keyword_config(
    conn: &Connection,
    project_id: i64,
    config: &KeywordConfig,
) -> Result<()> {
    let json = serde_json::to_string(config)?;
    set_project_setting(conn, project_id, KEYWORD_CONFIG_KEY, &json)
}

/// The project's CSV column layouts as raw JSON, or `None` if none were saved.
pub fn get_csv_layouts(conn: &Connection, project_id: i64) -> Result<Option<String>> {
    get_project_setting(conn, project_id, CSV_LAYOUTS_KEY)
}

pub fn set_csv_layouts(conn: &Connection, project_id: i64, json: &str) -> Result<()> {
    // Reject malformed JSON here rather than failing on the next load.
    serde_json::from_str::<serde_json::Value>(json)?;
    set_project_setting(conn, project_id, CSV_LAYOUTS_KEY, json)
}

/// The per-project video (ffmpeg) configuration (SPHIN-31), or `None` if the
/// project has never had one saved (callers should fall back to
/// [`VideoConfig::default`]).
pub fn get_video_config(conn: &Connection, project_id: i64) -> Result<Option<VideoConfig>> {
    match get_project_setting(conn, project_id, VIDEO_CONFIG_KEY)? {
        Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}

pub fn set_video_config(conn: &Connection, project_id: i64, config: &VideoConfig) -> Result<()> {
    let json = serde_json::to_string(config)?;
    set_project_setting(conn, project_id, VIDEO_CONFIG_KEY, &json)
}

/// The per-project transcription configuration (SPHIN-32), or `None` if the
/// project has never had one saved (callers should fall back to
/// [`TranscriptionConfig::default`], which is disabled).
pub fn get_transcription_config(
    conn: &Connection,
    project_id: i64,
) -> Result<Option<TranscriptionConfig>> {
    match get_project_setting(conn, project_id, TRANSCRIPTION_CONFIG_KEY)? {
        Some(json) => Ok(Some(serde_json::from_str(&json)?)),
        None => Ok(None),
    }
}

pub fn set_transcription_config(
    conn: &Connection,
    project_id: i64,
    config: &TranscriptionConfig,
) -> Result<()> {
    let json = serde_json::to_string(config)?;
    set_project_setting(conn, project_id, TRANSCRIPTION_CONFIG_KEY, &json)
}

// --- keyword cache (SPHIN-24) -------------------------------------------------

fn normalize_seed(seed: &str) -> String {
    seed.trim().to_lowercase()
}

/// Cached keyword suggestions for `seed` from `provider`, if fetched within
/// the last `ttl_days` days. A missing entry, an unparsable one, or one past
/// its TTL are all treated as a plain cache miss (`Ok(None)`) rather than an
/// error -- the caller should just fetch fresh.
pub fn get_cached_keywords(
    conn: &Connection,
    seed: &str,
    provider: &str,
    ttl_days: i64,
) -> Result<Option<Vec<String>>> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT keywords_json, fetched_at FROM keyword_cache WHERE seed = ?1 AND provider = ?2",
            params![normalize_seed(seed), provider],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    let Some((json, fetched_at)) = row else {
        return Ok(None);
    };
    let Ok(fetched) = chrono::DateTime::parse_from_rfc3339(&fetched_at) else {
        return Ok(None);
    };
    let age = Utc::now().signed_duration_since(fetched.with_timezone(&Utc));
    if age > chrono::Duration::days(ttl_days) {
        return Ok(None);
    }
    Ok(serde_json::from_str(&json).ok())
}

pub fn set_cached_keywords(
    conn: &Connection,
    seed: &str,
    provider: &str,
    keywords: &[String],
) -> Result<()> {
    let json = serde_json::to_string(keywords)?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO keyword_cache (seed, provider, keywords_json, fetched_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(seed, provider) DO UPDATE SET
            keywords_json = excluded.keywords_json,
            fetched_at = excluded.fetched_at",
        params![normalize_seed(seed), provider, json, now],
    )?;
    Ok(())
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

// --- per-site metadata drafts ------------------------------------------------

/// One site's edited metadata for an asset.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SiteMetadata {
    pub site_name: String,
    pub metadata: GeneratedMetadata,
    pub updated_at: String,
}

/// Insert or replace `site_name`'s draft for an asset. An already-embedded
/// asset drops back to `metadata_generated`, since its file no longer matches
/// what's been edited; uploaded assets keep their status.
pub fn upsert_site_metadata(
    conn: &Connection,
    asset_id: i64,
    site_name: &str,
    meta: &GeneratedMetadata,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO site_metadata (asset_id, site_name, metadata_json, updated_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(asset_id, site_name) DO UPDATE SET metadata_json = excluded.metadata_json, updated_at = excluded.updated_at",
        params![asset_id, site_name, serde_json::to_string(meta)?, now],
    )?;
    conn.execute(
        "UPDATE assets SET status = 'metadata_generated', updated_at = ?2 WHERE id = ?1 AND status = 'embedded'",
        params![asset_id, now],
    )?;
    Ok(())
}

pub fn list_site_metadata(conn: &Connection, asset_id: i64) -> Result<Vec<SiteMetadata>> {
    let mut stmt = conn.prepare(
        "SELECT site_name, metadata_json, updated_at FROM site_metadata WHERE asset_id = ?1 ORDER BY site_name",
    )?;
    let rows = stmt.query_map(params![asset_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (site_name, json, updated_at) = row?;
        out.push(SiteMetadata { site_name, metadata: serde_json::from_str(&json)?, updated_at });
    }
    Ok(out)
}

pub fn get_site_metadata(conn: &Connection, asset_id: i64, site_name: &str) -> Result<Option<GeneratedMetadata>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT metadata_json FROM site_metadata WHERE asset_id = ?1 AND site_name = ?2",
            params![asset_id, site_name],
            |r| r.get(0),
        )
        .optional()?;
    Ok(match json {
        Some(j) => Some(serde_json::from_str(&j)?),
        None => None,
    })
}

pub fn delete_site_metadata(conn: &Connection, asset_id: i64, site_name: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM site_metadata WHERE asset_id = ?1 AND site_name = ?2",
        params![asset_id, site_name],
    )?;
    Ok(())
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

fn row_to_job(row: &rusqlite::Row) -> rusqlite::Result<Job> {
    Ok(Job {
        id: row.get(0)?,
        asset_id: row.get(1)?,
        job_type: row.get(2)?,
        status: row.get(3)?,
        error: row.get(4)?,
        source: row.get(5)?,
        payload_json: row.get(6)?,
        attempts: row.get(7)?,
        not_before: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn row_to_sftp_profile(row: &rusqlite::Row) -> rusqlite::Result<SftpProfileRecord> {
    Ok(SftpProfileRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        site: row.get(3)?,
        protocol: row.get(4)?,
        host: row.get(5)?,
        port: row.get(6)?,
        username: row.get(7)?,
        remote_dir: row.get(8)?,
        credential_key: row.get(9)?,
        host_key_fingerprint: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
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
    fn delete_asset_removes_it_and_cascades_related_rows() {
        let conn = open_in_memory().unwrap();
        let a = insert_asset(&conn, "/tmp/a.jpg", "hash123", 42, "image").unwrap();
        insert_job(&conn, a.id, "analysis").unwrap();

        delete_asset(&conn, a.id).unwrap();

        assert!(get_asset(&conn, a.id).unwrap().is_none());
        assert_eq!(count_assets(&conn).unwrap(), 0);
        let job_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM jobs WHERE asset_id = ?1", [a.id], |r| r.get(0))
            .unwrap();
        assert_eq!(job_count, 0, "deleting an asset should cascade-delete its jobs");
    }

    #[test]
    fn deleting_an_unknown_asset_is_not_an_error() {
        let conn = open_in_memory().unwrap();
        assert!(delete_asset(&conn, 999).is_ok());
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
    fn saving_one_provider_does_not_clobber_another() {
        let conn = open_in_memory().unwrap();

        let mut openai = AnalysisConfig::new(ProviderKind::OpenAi);
        openai.api_key = "openai-key".into();
        set_analysis_config(&conn, 1, &openai).unwrap();

        let mut gemini = AnalysisConfig::new(ProviderKind::Gemini);
        gemini.api_key = "gemini-key".into();
        set_analysis_config(&conn, 1, &gemini).unwrap();

        // The most recently saved config is the active one...
        let active = get_analysis_config(&conn, 1).unwrap().unwrap();
        assert_eq!(active.provider, ProviderKind::Gemini);
        assert_eq!(active.api_key, "gemini-key");

        // ...but the OpenAI config saved earlier is still there.
        let all = get_analysis_configs(&conn, 1).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all["openai"].api_key, "openai-key");
        assert_eq!(all["gemini"].api_key, "gemini-key");

        // Switching back to OpenAI as active doesn't lose Gemini either.
        set_analysis_config(&conn, 1, &openai).unwrap();
        let all = get_analysis_configs(&conn, 1).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all["gemini"].api_key, "gemini-key");
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
    fn custom_site_profiles_can_be_added_and_removed() {
        let conn = open_in_memory().unwrap();
        assert!(get_custom_site_profiles(&conn, 1).unwrap().is_empty());

        let profiles = add_custom_site_profile(&conn, 1, LimiterProfile::custom("My Site")).unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "My Site");

        // Re-adding the same name replaces rather than duplicates.
        let mut edited = LimiterProfile::custom("My Site");
        edited.max_keywords = 10;
        let profiles = add_custom_site_profile(&conn, 1, edited).unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].max_keywords, 10);

        let profiles = remove_custom_site_profile(&conn, 1, "My Site").unwrap();
        assert!(profiles.is_empty());
        assert!(get_custom_site_profiles(&conn, 1).unwrap().is_empty());
    }

    #[test]
    fn built_in_site_profiles_can_be_deleted_and_restored() {
        let conn = open_in_memory().unwrap();
        let all = LimiterProfile::built_ins().len();
        assert_eq!(list_site_profiles(&conn, 1).unwrap().len(), all);

        let profiles = remove_site_profile(&conn, 1, "Shutterstock").unwrap();
        assert_eq!(profiles.len(), all - 1);
        assert!(!profiles.iter().any(|p| p.name == "Shutterstock"));

        // Re-adding a deleted built-in restores it rather than creating a custom copy.
        let profiles = add_site_profile(&conn, 1, LimiterProfile::custom("shutterstock")).unwrap();
        assert_eq!(profiles.len(), all);
        assert!(get_custom_site_profiles(&conn, 1).unwrap().is_empty());

        // A built-in that's still visible can't be added twice.
        assert!(add_site_profile(&conn, 1, LimiterProfile::custom("Pond5")).is_err());

        let profiles = add_site_profile(&conn, 1, LimiterProfile::custom("My Site")).unwrap();
        assert_eq!(profiles.len(), all + 1);
        let profiles = remove_site_profile(&conn, 1, "My Site").unwrap();
        assert_eq!(profiles.len(), all);
    }

    #[test]
    fn custom_site_profile_rejects_blank_or_built_in_names() {
        let conn = open_in_memory().unwrap();
        assert!(add_custom_site_profile(&conn, 1, LimiterProfile::custom("  ")).is_err());
        assert!(add_custom_site_profile(&conn, 1, LimiterProfile::custom("shutterstock")).is_err());
        assert!(get_custom_site_profiles(&conn, 1).unwrap().is_empty());
    }

    #[test]
    fn embed_config_roundtrips_per_project() {
        let conn = open_in_memory().unwrap();
        assert!(get_embed_config(&conn, 1).unwrap().is_none());

        let config = EmbedConfig {
            exiftool_path: "C:\\Tools\\exiftool.exe".into(),
        };
        set_embed_config(&conn, 1, &config).unwrap();

        let back = get_embed_config(&conn, 1).unwrap().unwrap();
        assert_eq!(back.exiftool_path, "C:\\Tools\\exiftool.exe");
    }

    #[test]
    fn enqueued_jobs_are_pending_and_source_queue() {
        let conn = open_in_memory().unwrap();
        let a = insert_asset(&conn, "/tmp/a.jpg", "h", 1, "image").unwrap();
        let jobs = enqueue_jobs(&conn, &[a.id], "analyze", r#"{"project_id":1}"#).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, "pending");
        assert_eq!(jobs[0].source, "queue");
        assert_eq!(jobs[0].payload_json, r#"{"project_id":1}"#);
    }

    #[test]
    fn claim_next_pending_job_only_claims_queue_jobs_oldest_first() {
        let conn = open_in_memory().unwrap();
        let a = insert_asset(&conn, "/tmp/a.jpg", "h", 1, "image").unwrap();
        // A direct-command job sitting pending must never be claimed.
        insert_job(&conn, a.id, "analysis").unwrap();
        let queued = enqueue_jobs(&conn, &[a.id, a.id], "analyze", "{}").unwrap();

        let claimed = claim_next_pending_job(&conn).unwrap().unwrap();
        assert_eq!(claimed.id, queued[0].id);
        assert_eq!(claimed.status, "running");

        let claimed2 = claim_next_pending_job(&conn).unwrap().unwrap();
        assert_eq!(claimed2.id, queued[1].id);

        assert!(claim_next_pending_job(&conn).unwrap().is_none());
    }

    #[test]
    fn retry_job_resets_a_failed_queue_job_to_pending() {
        let conn = open_in_memory().unwrap();
        let a = insert_asset(&conn, "/tmp/a.jpg", "h", 1, "image").unwrap();
        let job = &enqueue_jobs(&conn, &[a.id], "analyze", "{}").unwrap()[0];
        claim_next_pending_job(&conn).unwrap();
        set_job_status(&conn, job.id, "failed", Some("boom")).unwrap();

        retry_job(&conn, job.id).unwrap();

        let refreshed = claim_next_pending_job(&conn).unwrap().unwrap();
        assert_eq!(refreshed.id, job.id);
        assert_eq!(refreshed.attempts, 1);
        assert!(refreshed.error.is_none());
    }

    #[test]
    fn cancel_pending_job_marks_it_cancelled_and_it_is_never_claimed() {
        let conn = open_in_memory().unwrap();
        let a = insert_asset(&conn, "/tmp/a.jpg", "h", 1, "image").unwrap();
        let jobs = enqueue_jobs(&conn, &[a.id, a.id], "upload", "{}").unwrap();

        assert!(cancel_pending_job(&conn, jobs[0].id).unwrap());

        let claimed = claim_next_pending_job(&conn).unwrap().unwrap();
        assert_eq!(claimed.id, jobs[1].id, "the cancelled job must never be claimed");

        let counts = queue_job_counts(&conn).unwrap();
        assert_eq!(counts.cancelled, 1);
    }

    #[test]
    fn cancel_pending_job_is_a_no_op_on_a_job_that_already_finished() {
        let conn = open_in_memory().unwrap();
        let a = insert_asset(&conn, "/tmp/a.jpg", "h", 1, "image").unwrap();
        let job = &enqueue_jobs(&conn, &[a.id], "analyze", "{}").unwrap()[0];
        claim_next_pending_job(&conn).unwrap();
        set_job_status(&conn, job.id, "done", None).unwrap();

        assert!(!cancel_pending_job(&conn, job.id).unwrap());
        let jobs = list_queue_jobs(&conn, 10).unwrap();
        assert_eq!(jobs[0].status, "done");
    }

    #[test]
    fn site_metadata_round_trips_and_unembeds_the_asset() {
        let conn = open_in_memory().unwrap();
        let a = insert_asset(&conn, "/tmp/a.jpg", "h", 1, "image").unwrap();
        set_asset_status(&conn, a.id, "embedded").unwrap();
        let meta = GeneratedMetadata {
            title: "Sunset".into(),
            description: "Over the sea".into(),
            keywords: vec!["sunset".into(), "sea".into()],
            profile: "Adobe Stock".into(),
            meets_minimum_keywords: true,
        };
        upsert_site_metadata(&conn, a.id, "Adobe Stock", &meta).unwrap();
        let edited = GeneratedMetadata { title: "Sunset 2".into(), ..meta.clone() };
        upsert_site_metadata(&conn, a.id, "Adobe Stock", &edited).unwrap();

        let all = list_site_metadata(&conn, a.id).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].metadata.title, "Sunset 2");
        assert_eq!(get_asset(&conn, a.id).unwrap().unwrap().status, "metadata_generated");

        delete_site_metadata(&conn, a.id, "Adobe Stock").unwrap();
        assert!(get_site_metadata(&conn, a.id, "Adobe Stock").unwrap().is_none());
    }

    #[test]
    fn delete_finished_jobs_removes_only_terminal_jobs() {
        let conn = open_in_memory().unwrap();
        let a = insert_asset(&conn, "/tmp/a.jpg", "h", 1, "image").unwrap();
        let jobs = enqueue_jobs(&conn, &[a.id, a.id, a.id], "analyze", "{}").unwrap();
        set_job_status(&conn, jobs[0].id, "done", None).unwrap();
        set_job_status(&conn, jobs[1].id, "failed", Some("boom")).unwrap();

        // A pending job is never removed, even when named explicitly.
        assert_eq!(delete_finished_jobs(&conn, Some(&[jobs[2].id]), None).unwrap(), 0);
        assert_eq!(delete_finished_jobs(&conn, None, Some("done")).unwrap(), 1);
        assert!(list_job_events(&conn, jobs[0].id).unwrap().is_empty());

        let counts = queue_job_counts(&conn).unwrap();
        assert_eq!(counts.done, 0);
        assert_eq!(counts.failed, 1);
        assert_eq!(counts.pending, 1);
    }

    #[test]
    fn queue_job_counts_tally_by_status() {
        let conn = open_in_memory().unwrap();
        let a = insert_asset(&conn, "/tmp/a.jpg", "h", 1, "image").unwrap();
        let jobs = enqueue_jobs(&conn, &[a.id, a.id, a.id], "analyze", "{}").unwrap();
        claim_next_pending_job(&conn).unwrap(); // jobs[0] -> running
        set_job_status(&conn, jobs[1].id, "done", None).unwrap();

        let counts = queue_job_counts(&conn).unwrap();
        assert_eq!(counts.pending, 1);
        assert_eq!(counts.running, 1);
        assert_eq!(counts.done, 1);
        assert_eq!(counts.failed, 0);
    }

    #[test]
    fn list_queue_jobs_excludes_direct_jobs() {
        let conn = open_in_memory().unwrap();
        let a = insert_asset(&conn, "/tmp/a.jpg", "h", 1, "image").unwrap();
        insert_job(&conn, a.id, "analysis").unwrap();
        enqueue_jobs(&conn, &[a.id], "analyze", "{}").unwrap();

        let jobs = list_queue_jobs(&conn, 10).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].source, "queue");
    }

    #[test]
    fn schedule_retry_sets_a_future_not_before_and_keeps_the_job_uncclaimable_until_then() {
        let conn = open_in_memory().unwrap();
        let a = insert_asset(&conn, "/tmp/a.jpg", "h", 1, "image").unwrap();
        let job = &enqueue_jobs(&conn, &[a.id], "upload", "{}").unwrap()[0];
        claim_next_pending_job(&conn).unwrap();

        schedule_retry(&conn, job.id, 3600, "transient network error").unwrap();

        // Not eligible yet: the backoff window hasn't passed.
        assert!(claim_next_pending_job(&conn).unwrap().is_none());

        // Force the window into the past and it becomes claimable again.
        conn.execute(
            "UPDATE jobs SET not_before = ?1 WHERE id = ?2",
            params![(Utc::now() - chrono::Duration::seconds(1)).to_rfc3339(), job.id],
        )
        .unwrap();
        let claimed = claim_next_pending_job(&conn).unwrap().unwrap();
        assert_eq!(claimed.id, job.id);
        assert_eq!(claimed.attempts, 1);
        assert_eq!(claimed.error.as_deref(), Some("transient network error"));
    }

    #[test]
    fn sftp_profile_crud_roundtrips_and_derives_a_credential_key() {
        let conn = open_in_memory().unwrap();
        assert!(list_sftp_profiles(&conn, 1).unwrap().is_empty());

        let profile = create_sftp_profile(&conn, 1, "Adobe Stock", "adobe_stock", "sftp", "sftp.adobe.io", 22, "alice", "/incoming")
            .unwrap();
        assert_eq!(profile.credential_key, format!("sftp-profile-{}", profile.id));

        let fetched = get_sftp_profile(&conn, profile.id).unwrap().unwrap();
        assert_eq!(fetched.host, "sftp.adobe.io");
        assert!(fetched.host_key_fingerprint.is_none());

        set_sftp_host_key_fingerprint(&conn, profile.id, "abc123").unwrap();
        update_sftp_profile(&conn, profile.id, "Adobe Stock (renamed)", "adobe_stock", "sftp", "sftp.adobe.io", 22, "alice", "/incoming/new")
            .unwrap();
        let updated = get_sftp_profile(&conn, profile.id).unwrap().unwrap();
        assert_eq!(updated.name, "Adobe Stock (renamed)");
        assert_eq!(updated.remote_dir, "/incoming/new");
        assert_eq!(updated.host_key_fingerprint.as_deref(), Some("abc123"));

        assert_eq!(list_sftp_profiles(&conn, 1).unwrap().len(), 1);

        delete_sftp_profile(&conn, profile.id).unwrap();
        assert!(get_sftp_profile(&conn, profile.id).unwrap().is_none());
    }

    #[test]
    fn keyword_config_roundtrips_per_project() {
        use crate::keywords::{KeywordConfig, SiteCredentials};

        let conn = open_in_memory().unwrap();
        assert!(get_keyword_config(&conn, 1).unwrap().is_none());

        let config = KeywordConfig {
            shutterstock: Some(SiteCredentials {
                api_key: "sk-1".into(),
                base_url: String::new(),
            }),
            adobe_stock: None,
        };
        set_keyword_config(&conn, 1, &config).unwrap();

        let back = get_keyword_config(&conn, 1).unwrap().unwrap();
        assert_eq!(back.shutterstock.unwrap().api_key, "sk-1");
        assert!(back.adobe_stock.is_none());
    }

    #[test]
    fn video_config_roundtrips_per_project() {
        let conn = open_in_memory().unwrap();
        assert!(get_video_config(&conn, 1).unwrap().is_none());

        let config = VideoConfig {
            ffmpeg_path: "C:\\Tools\\ffmpeg.exe".into(),
            max_keyframes: 10,
            scene_threshold: 0.25,
        };
        set_video_config(&conn, 1, &config).unwrap();

        let back = get_video_config(&conn, 1).unwrap().unwrap();
        assert_eq!(back.ffmpeg_path, "C:\\Tools\\ffmpeg.exe");
        assert_eq!(back.max_keyframes, 10);
    }

    #[test]
    fn transcription_config_roundtrips_per_project() {
        let conn = open_in_memory().unwrap();
        assert!(get_transcription_config(&conn, 1).unwrap().is_none());

        let config = TranscriptionConfig {
            enabled: true,
            api_key: "sk-whisper".into(),
            base_url: String::new(),
            model: "whisper-1".into(),
        };
        set_transcription_config(&conn, 1, &config).unwrap();

        let back = get_transcription_config(&conn, 1).unwrap().unwrap();
        assert!(back.enabled);
        assert_eq!(back.api_key, "sk-whisper");
    }

    #[test]
    fn keyword_cache_roundtrips_and_normalizes_the_seed() {
        let conn = open_in_memory().unwrap();
        assert!(get_cached_keywords(&conn, "Dog Park", "shutterstock", 30)
            .unwrap()
            .is_none());

        let kw = vec!["dog".to_string(), "park".to_string()];
        set_cached_keywords(&conn, "Dog Park", "shutterstock", &kw).unwrap();

        // Case/whitespace-insensitive lookup.
        let back = get_cached_keywords(&conn, "  dog park  ", "shutterstock", 30)
            .unwrap()
            .unwrap();
        assert_eq!(back, kw);

        // A different provider for the same seed is a separate entry.
        assert!(get_cached_keywords(&conn, "Dog Park", "adobe_stock", 30)
            .unwrap()
            .is_none());
    }

    #[test]
    fn keyword_cache_expires_past_its_ttl() {
        let conn = open_in_memory().unwrap();
        let stale = Utc::now() - chrono::Duration::days(40);
        conn.execute(
            "INSERT INTO keyword_cache (seed, provider, keywords_json, fetched_at)
             VALUES ('dog', 'shutterstock', '[\"dog\"]', ?1)",
            params![stale.to_rfc3339()],
        )
        .unwrap();

        assert!(get_cached_keywords(&conn, "dog", "shutterstock", 30)
            .unwrap()
            .is_none());
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
