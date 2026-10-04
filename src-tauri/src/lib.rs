// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde::Serialize;
use sphinx_core::analysis::{AnalysisConfig, AnalysisResult};
use sphinx_core::db::JobCounts;
use sphinx_core::embed::{EmbedConfig, EmbedOutcome, ExportRow};
use sphinx_core::keywords::{
    AdobeStockProvider, KeywordConfig, KeywordProvider, ShutterstockProvider, SiteCredentials,
};
use sphinx_core::metadata::{GeneratedMetadata, LimiterProfile};
use sphinx_core::models::{
    AnalysisRecord, Asset, IngestOutcome, Job, JobEvent, MetadataRecord, Project, SftpProfileRecord,
};
use sphinx_core::secrets;
use sphinx_core::transcribe::TranscriptionConfig;
use sphinx_core::upload::{SftpProfile, SftpSite, TransportProtocol};
use sphinx_core::video::VideoConfig;
use sphinx_core::watch::WatchHandle;
use sphinx_core::{analysis, db, embed, ingest, metadata, upload, video};
use tauri::{Emitter, Manager};

/// Shared app state: the SQLite connection and the current folder-watch
/// handle (if any). A single connection behind a mutex is plenty for the
/// ingestion workload here (no long-running writes): the batch job worker
/// (SPHIN-7) processes one queue job at a time rather than needing
/// concurrent writers, so this doesn't need to grow into a connection pool.
struct AppState {
    db: Mutex<Connection>,
    watch: Mutex<Option<WatchHandle>>,
    /// The queue job the worker is currently executing (id + a handle to wake
    /// it for cancellation), if any. `None` between jobs.
    running_job: Mutex<Option<(i64, Arc<tokio::sync::Notify>)>>,
}

#[derive(Debug, Serialize, Clone)]
struct IngestSummary {
    ingested: usize,
    duplicates: usize,
    skipped: usize,
    errors: usize,
    outcomes: Vec<IngestOutcome>,
}

fn summarize(results: Vec<sphinx_core::Result<IngestOutcome>>) -> IngestSummary {
    let mut summary = IngestSummary {
        ingested: 0,
        duplicates: 0,
        skipped: 0,
        errors: 0,
        outcomes: Vec::new(),
    };
    for r in results {
        match r {
            Ok(outcome) => {
                match &outcome {
                    IngestOutcome::Ingested { .. } => summary.ingested += 1,
                    IngestOutcome::Duplicate { .. } => summary.duplicates += 1,
                    IngestOutcome::Skipped { .. } => summary.skipped += 1,
                }
                summary.outcomes.push(outcome);
            }
            Err(_) => summary.errors += 1,
        }
    }
    summary
}

// --- ingestion (SPHIN-1) -------------------------------------------------------

/// Ingest an explicit list of file paths (from the native file picker or a
/// drag-and-drop event). SPHIN-11.
///
/// Runs on its own short-lived connection (like the folder-watch callback
/// does) rather than holding `state.db`'s mutex for the whole batch --
/// hashing a large batch can take a while, and hogging the app's single
/// shared connection for that long made every other command (queue polling,
/// simple reads) queue up behind it, which is what made the app feel hung
/// during a big import.
#[tauri::command]
async fn ingest_files(app: tauri::AppHandle, paths: Vec<String>) -> Result<IngestSummary, String> {
    let db_path = app.path().app_data_dir().map_err(|e| e.to_string())?.join("sphinx.db");
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open(&db_path).map_err(|e| e.to_string())?;
        let results = ingest::ingest_paths(&conn, paths);
        Ok::<_, String>(summarize(results))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Recursively ingest every supported file in a folder (used both for a
/// one-off "import this folder" action and to backfill when watch mode is
/// first turned on for a folder). SPHIN-11 / SPHIN-12. See [`ingest_files`]
/// for why this uses its own connection instead of `state.db`.
#[tauri::command]
async fn ingest_folder(app: tauri::AppHandle, dir: String) -> Result<IngestSummary, String> {
    let db_path = app.path().app_data_dir().map_err(|e| e.to_string())?.join("sphinx.db");
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open(&db_path).map_err(|e| e.to_string())?;
        let results = ingest::ingest_directory(&conn, &dir);
        Ok::<_, String>(summarize(results))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Start watching a folder for new files, ingesting each one as it appears
/// and pushing an `assets-ingested` event to the frontend. SPHIN-12.
#[tauri::command]
fn start_watch(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    dir: String,
) -> Result<(), String> {
    // Stop any previous watch first -- only one active watch folder at a time
    // for now (multi-folder watch can be layered on later without changing
    // this command's shape).
    if let Ok(mut current) = state.watch.lock() {
        if let Some(handle) = current.take() {
            handle.stop();
        }
    }

    let db_path = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("sphinx.db");
    let app_for_events = app.clone();

    let handle = sphinx_core::watch::watch_folder(&dir, move |paths| {
        // Each watch callback runs on its own background thread, so it opens
        // its own short-lived connection rather than fighting the app's
        // Mutex-guarded connection for the whole watch lifetime.
        let conn = match db::open(&db_path) {
            Ok(c) => c,
            Err(_) => return,
        };
        let results = ingest::ingest_paths(&conn, paths);
        let summary = summarize(results);
        let _ = app_for_events.emit("assets-ingested", summary);
    })
    .map_err(|e| e.to_string())?;

    *state.watch.lock().map_err(|e| e.to_string())? = Some(handle);
    Ok(())
}

/// Stop the active folder watch, if any. SPHIN-12.
#[tauri::command]
fn stop_watch(state: tauri::State<AppState>) -> Result<(), String> {
    if let Some(handle) = state.watch.lock().map_err(|e| e.to_string())?.take() {
        handle.stop();
    }
    Ok(())
}

/// List ingested assets, most recent first. SPHIN-14.
#[tauri::command]
fn list_assets(state: tauri::State<AppState>, limit: i64, offset: i64) -> Result<Vec<Asset>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::list_assets(&conn, limit, offset).map_err(|e| e.to_string())
}

#[tauri::command]
fn asset_count(state: tauri::State<AppState>) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::count_assets(&conn).map_err(|e| e.to_string())
}

/// Remove an asset from the library (catalog row + cascaded jobs/analyses/
/// metadata only -- the original file on disk is untouched).
#[tauri::command]
fn delete_asset(state: tauri::State<AppState>, asset_id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::delete_asset(&conn, asset_id).map_err(|e| e.to_string())
}

// --- API keys in the OS credential store --------------------------------------
//
// Provider configs live in `project_settings`, but their API keys don't: every
// save moves the key into the OS credential store (see `secrets`) and writes
// the config with it blanked; every load fills it back in.

/// Credential-store name for one of a project's API keys.
fn api_key_name(project_id: i64, purpose: &str) -> String {
    format!("project-{project_id}-{purpose}")
}

/// Move `key` into the credential store under `name` and blank it so it's
/// never written to `sphinx.db`. An empty key drops any stored one.
fn stash_api_key(name: &str, key: &mut String) -> Result<(), String> {
    if key.trim().is_empty() {
        // Best effort: a provider with no key (Ollama) shouldn't need a
        // working credential store just to be saved.
        let _ = secrets::delete_api_key(name);
    } else {
        secrets::set_api_key(name, key.trim()).map_err(|e| e.to_string())?;
    }
    key.clear();
    Ok(())
}

/// Fill a blank `key` from the credential store. A key still in the config
/// is a legacy plaintext one the startup migration couldn't move: keep it.
/// An unreadable store just leaves the key empty, so the provider reports a
/// missing key rather than the whole settings screen failing to load.
fn fill_api_key(name: &str, key: &mut String) {
    if key.is_empty() {
        if let Ok(Some(stored)) = secrets::get_api_key(name) {
            *key = stored;
        }
    }
}

fn analysis_key_name(project_id: i64, config: &AnalysisConfig) -> String {
    api_key_name(project_id, &format!("analysis-{}", config.provider.as_str()))
}

fn stash_analysis_key(project_id: i64, config: &mut AnalysisConfig) -> Result<(), String> {
    let name = analysis_key_name(project_id, config);
    stash_api_key(&name, &mut config.api_key)
}

fn keyword_slots(config: &mut KeywordConfig) -> [(&'static str, &mut Option<SiteCredentials>); 2] {
    [
        ("keywords-shutterstock", &mut config.shutterstock),
        ("keywords-adobe-stock", &mut config.adobe_stock),
    ]
}

fn load_analysis_config(conn: &Connection, project_id: i64) -> Result<Option<AnalysisConfig>, String> {
    let mut config = db::get_analysis_config(conn, project_id).map_err(|e| e.to_string())?;
    if let Some(c) = config.as_mut() {
        fill_api_key(&analysis_key_name(project_id, c), &mut c.api_key);
    }
    Ok(config)
}

fn load_analysis_configs(
    conn: &Connection,
    project_id: i64,
) -> Result<std::collections::HashMap<String, AnalysisConfig>, String> {
    let mut configs = db::get_analysis_configs(conn, project_id).map_err(|e| e.to_string())?;
    for c in configs.values_mut() {
        fill_api_key(&analysis_key_name(project_id, c), &mut c.api_key);
    }
    Ok(configs)
}

fn load_keyword_config(conn: &Connection, project_id: i64) -> Result<Option<KeywordConfig>, String> {
    let mut config = db::get_keyword_config(conn, project_id).map_err(|e| e.to_string())?;
    if let Some(cfg) = config.as_mut() {
        for (purpose, creds) in keyword_slots(cfg) {
            if let Some(c) = creds {
                fill_api_key(&api_key_name(project_id, purpose), &mut c.api_key);
            }
        }
    }
    Ok(config)
}

fn load_transcription_config(conn: &Connection, project_id: i64) -> Result<Option<TranscriptionConfig>, String> {
    let mut config = db::get_transcription_config(conn, project_id).map_err(|e| e.to_string())?;
    if let Some(c) = config.as_mut() {
        fill_api_key(&api_key_name(project_id, "transcription"), &mut c.api_key);
    }
    Ok(config)
}

/// Move API keys that earlier versions saved in plaintext into the credential
/// store. Runs at every startup but only acts on non-empty stored keys. A key
/// that can't be moved stays in the database (and keeps working) rather than
/// being lost; the next launch tries again.
fn migrate_plaintext_api_keys(conn: &Connection) {
    let Ok(projects) = db::list_projects(conn) else { return };
    for project in projects {
        let pid = project.id;

        // The active config first: saving it also rewrites its entry in the
        // per-provider map, so the loop below won't see that key again.
        if let Ok(Some(mut active)) = db::get_analysis_config(conn, pid) {
            if !active.api_key.is_empty() && stash_analysis_key(pid, &mut active).is_ok() {
                let _ = db::set_analysis_config(conn, pid, &active);
            }
        }
        if let Ok(mut configs) = db::get_analysis_configs(conn, pid) {
            let mut changed = false;
            for c in configs.values_mut() {
                if !c.api_key.is_empty() && stash_analysis_key(pid, c).is_ok() {
                    changed = true;
                }
            }
            if changed {
                let _ = db::set_analysis_configs(conn, pid, &configs);
            }
        }

        if let Ok(Some(mut cfg)) = db::get_keyword_config(conn, pid) {
            let mut changed = false;
            for (purpose, creds) in keyword_slots(&mut cfg) {
                if let Some(c) = creds {
                    if !c.api_key.is_empty() && stash_api_key(&api_key_name(pid, purpose), &mut c.api_key).is_ok() {
                        changed = true;
                    }
                }
            }
            if changed {
                let _ = db::set_keyword_config(conn, pid, &cfg);
            }
        }

        if let Ok(Some(mut cfg)) = db::get_transcription_config(conn, pid) {
            if !cfg.api_key.is_empty()
                && stash_api_key(&api_key_name(pid, "transcription"), &mut cfg.api_key).is_ok()
            {
                let _ = db::set_transcription_config(conn, pid, &cfg);
            }
        }
    }
}

// --- projects & analysis config (SPHIN-2 / SPHIN-17) --------------------------

#[tauri::command]
fn list_projects(state: tauri::State<AppState>) -> Result<Vec<Project>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::list_projects(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
fn create_project(state: tauri::State<AppState>, name: String) -> Result<Project, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::create_project(&conn, name.trim()).map_err(|e| e.to_string())
}

/// The saved vision-provider config for a project, or `null` if never set.
#[tauri::command]
fn get_analysis_config(
    state: tauri::State<AppState>,
    project_id: i64,
) -> Result<Option<AnalysisConfig>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    load_analysis_config(&conn, project_id)
}

#[tauri::command]
fn set_analysis_config(
    state: tauri::State<AppState>,
    project_id: i64,
    mut config: AnalysisConfig,
) -> Result<(), String> {
    stash_analysis_key(project_id, &mut config)?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::set_analysis_config(&conn, project_id, &config).map_err(|e| e.to_string())
}

/// Every vision-provider config a project has saved, keyed by provider name
/// ("openai", "gemini", "anthropic") -- lets the UI offer switching providers
/// without losing previously-entered keys/models for the others.
#[tauri::command]
fn get_analysis_configs(
    state: tauri::State<AppState>,
    project_id: i64,
) -> Result<std::collections::HashMap<String, AnalysisConfig>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    load_analysis_configs(&conn, project_id)
}

/// Check whether a local Ollama server is reachable, returning a short
/// summary of the models it has pulled. SPHIN-34. Reads the project's saved
/// "ollama" config if there is one (so the user can check connectivity
/// before switching the active provider to it), else a fresh default.
#[tauri::command]
async fn check_ollama(
    state: tauri::State<'_, AppState>,
    project_id: i64,
) -> Result<String, String> {
    let config = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        load_analysis_configs(&conn, project_id)?
            .get("ollama")
            .cloned()
            .unwrap_or_else(|| AnalysisConfig::new(analysis::ProviderKind::Ollama))
    };
    tauri::async_runtime::spawn_blocking(move || analysis::check_ollama(&config))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Best-effort local GPU capability check (SPHIN-36), surfaced as a warning
/// in Settings when running local models without a detected GPU.
#[tauri::command]
async fn detect_gpu() -> sphinx_core::gpu::GpuInfo {
    tauri::async_runtime::spawn_blocking(sphinx_core::gpu::detect_gpu)
        .await
        .unwrap_or(sphinx_core::gpu::GpuInfo {
            available: false,
            backend: "none".to_string(),
            detail: "detection failed".to_string(),
        })
}

#[derive(Debug, Serialize)]
struct AnalysisResponse {
    record_id: i64,
    result: AnalysisResult,
}

/// The latest stored analysis for an asset, parsed back into its structured
/// form, or `null` if the asset has never been analyzed. SPHIN-2.
#[tauri::command]
fn get_analysis(
    state: tauri::State<AppState>,
    asset_id: i64,
) -> Result<Option<AnalysisResponse>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let record: Option<AnalysisRecord> =
        db::latest_analysis_for_asset(&conn, asset_id).map_err(|e| e.to_string())?;
    match record {
        Some(r) => {
            let result: AnalysisResult =
                serde_json::from_str(&r.result_json).map_err(|e| e.to_string())?;
            Ok(Some(AnalysisResponse {
                record_id: r.id,
                result,
            }))
        }
        None => Ok(None),
    }
}

/// Run the project's configured vision model against one asset (image or
/// video), store the structured result, and advance the asset to
/// `analyzed`. SPHIN-2 (15/16/17) for images; SPHIN-8 (31/32/33) for video --
/// keyframes are sampled via ffmpeg and, if transcription is enabled, an
/// audio transcript is appended, before a single request to the same vision
/// provider images use.
///
/// The network call (and, for video, ffmpeg/transcription) is blocking, so it
/// runs on the blocking thread pool; the DB mutex is only held to read inputs
/// and write results, never across the call itself.
#[tauri::command]
async fn analyze_asset(
    state: tauri::State<'_, AppState>,
    project_id: i64,
    asset_id: i64,
) -> Result<AnalysisResponse, String> {
    let (config, asset, video_config, transcription_config) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let config = load_analysis_config(&conn, project_id)?
            .ok_or_else(|| "no analysis provider configured for this project".to_string())?;
        let asset = db::get_asset(&conn, asset_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("asset {asset_id} not found"))?;
        let video_config = db::get_video_config(&conn, project_id)
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let transcription_config = load_transcription_config(&conn, project_id)?;
        (config, asset, video_config, transcription_config)
    };

    let job = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::insert_job(&conn, asset_id, "analysis").map_err(|e| e.to_string())?
    };

    let path = asset.path.clone();
    let is_image = asset.media_type == "image";
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        if is_image {
            analysis::analyze_file(&config, &path)
        } else {
            analysis::analyze_video_file(&config, &video_config, transcription_config.as_ref(), &path)
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    match outcome {
        Ok(result) => {
            let json = serde_json::to_string(&result).map_err(|e| e.to_string())?;
            let record = db::insert_analysis(&conn, asset_id, &result.provider, &result.model, &json)
                .map_err(|e| e.to_string())?;
            db::set_job_status(&conn, job.id, "done", None).map_err(|e| e.to_string())?;
            db::set_asset_status(&conn, asset_id, "analyzed").map_err(|e| e.to_string())?;
            Ok(AnalysisResponse {
                record_id: record.id,
                result,
            })
        }
        Err(e) => {
            let msg = e.to_string();
            let _ = db::set_job_status(&conn, job.id, "failed", Some(&msg));
            Err(msg)
        }
    }
}

// --- video pipeline (SPHIN-8) -------------------------------------------------

#[tauri::command]
fn get_video_config(
    state: tauri::State<AppState>,
    project_id: i64,
) -> Result<Option<VideoConfig>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::get_video_config(&conn, project_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_video_config(
    state: tauri::State<AppState>,
    project_id: i64,
    config: VideoConfig,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::set_video_config(&conn, project_id, &config).map_err(|e| e.to_string())
}

/// Check whether ffmpeg is reachable at a project's configured path (or on
/// PATH), returning its version string. SPHIN-31.
#[tauri::command]
async fn check_ffmpeg(
    state: tauri::State<'_, AppState>,
    project_id: i64,
) -> Result<String, String> {
    let config = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::get_video_config(&conn, project_id)
            .map_err(|e| e.to_string())?
            .unwrap_or_default()
    };
    tauri::async_runtime::spawn_blocking(move || video::check_ffmpeg(&config))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_transcription_config(
    state: tauri::State<AppState>,
    project_id: i64,
) -> Result<Option<TranscriptionConfig>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    load_transcription_config(&conn, project_id)
}

#[tauri::command]
fn set_transcription_config(
    state: tauri::State<AppState>,
    project_id: i64,
    mut config: TranscriptionConfig,
) -> Result<(), String> {
    stash_api_key(&api_key_name(project_id, "transcription"), &mut config.api_key)?;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::set_transcription_config(&conn, project_id, &config).map_err(|e| e.to_string())
}

// --- metadata generation & limiter profiles (SPHIN-3) ------------------------

/// The built-in limiter profile presets, for a site picker in the UI.
#[tauri::command]
fn list_limiter_profiles() -> Vec<LimiterProfile> {
    LimiterProfile::built_ins()
}

/// The saved limiter profile for a project, or `null` if never set (the UI
/// should fall back to [`LimiterProfile::default`]).
#[tauri::command]
fn get_limiter_profile(
    state: tauri::State<AppState>,
    project_id: i64,
) -> Result<Option<LimiterProfile>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::get_limiter_profile(&conn, project_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_limiter_profile(
    state: tauri::State<AppState>,
    project_id: i64,
    profile: LimiterProfile,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::set_limiter_profile(&conn, project_id, &profile).map_err(|e| e.to_string())
}

/// Every site profile available to a project: the built-in presets it
/// hasn't deleted plus any custom ones it has added (SPHIN-19 follow-up).
#[tauri::command]
fn list_site_profiles(
    state: tauri::State<AppState>,
    project_id: i64,
) -> Result<Vec<LimiterProfile>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::list_site_profiles(&conn, project_id).map_err(|e| e.to_string())
}

/// Add (or edit, by re-adding with the same name) a custom site profile, or
/// restore a deleted built-in preset of the same name. Returns the project's
/// full visible site-profile list.
#[tauri::command]
fn add_site_profile(
    state: tauri::State<AppState>,
    project_id: i64,
    profile: LimiterProfile,
) -> Result<Vec<LimiterProfile>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::add_site_profile(&conn, project_id, profile).map_err(|e| e.to_string())
}

/// Delete a site profile by name -- built-in presets are hidden for the
/// project, custom ones removed. Returns the full visible list.
#[tauri::command]
fn remove_site_profile(
    state: tauri::State<AppState>,
    project_id: i64,
    name: String,
) -> Result<Vec<LimiterProfile>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::remove_site_profile(&conn, project_id, &name).map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
struct MetadataResponse {
    record_id: i64,
    result: GeneratedMetadata,
}

fn metadata_response(record: MetadataRecord) -> Result<MetadataResponse, String> {
    let keywords: Vec<String> =
        serde_json::from_str(&record.keywords_json).map_err(|e| e.to_string())?;
    Ok(MetadataResponse {
        record_id: record.id,
        result: GeneratedMetadata {
            title: record.title,
            description: record.description,
            keywords,
            profile: record.profile,
            meets_minimum_keywords: record.meets_minimum_keywords,
        },
    })
}

/// The latest generated metadata for an asset, or `null` if none has been
/// generated yet.
#[tauri::command]
fn get_metadata(
    state: tauri::State<AppState>,
    asset_id: i64,
) -> Result<Option<MetadataResponse>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    match db::latest_metadata_for_asset(&conn, asset_id).map_err(|e| e.to_string())? {
        Some(record) => metadata_response(record).map(Some),
        None => Ok(None),
    }
}

/// Generate title/description/keywords from the asset's latest analysis,
/// fit to the project's limiter profile (or [`LimiterProfile::default`] if
/// the project hasn't picked one). Pure and fast -- no network call. SPHIN-3
/// (18/19).
#[tauri::command]
fn generate_metadata(
    state: tauri::State<AppState>,
    project_id: i64,
    asset_id: i64,
) -> Result<MetadataResponse, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let analysis_record = db::latest_analysis_for_asset(&conn, asset_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "asset has not been analyzed yet".to_string())?;
    let analysis: AnalysisResult =
        serde_json::from_str(&analysis_record.result_json).map_err(|e| e.to_string())?;
    let profile = db::get_limiter_profile(&conn, project_id)
        .map_err(|e| e.to_string())?
        .unwrap_or_default();

    let generated = metadata::generate(&analysis, &profile);
    let record = db::insert_metadata(&conn, asset_id, &generated).map_err(|e| e.to_string())?;
    db::set_asset_status(&conn, asset_id, "metadata_generated").map_err(|e| e.to_string())?;
    metadata_response(record)
}

/// Persist a user-edited title/description/keyword set as the asset's latest
/// metadata, so a later `embed_asset_metadata` picks up the edit rather than
/// the last AI-generated draft. Used by the asset editor's "Save & embed".
#[tauri::command]
fn set_metadata(
    state: tauri::State<AppState>,
    asset_id: i64,
    metadata: GeneratedMetadata,
) -> Result<MetadataResponse, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let record = db::insert_metadata(&conn, asset_id, &metadata).map_err(|e| e.to_string())?;
    db::set_asset_status(&conn, asset_id, "metadata_generated").map_err(|e| e.to_string())?;
    metadata_response(record)
}

/// Every per-site metadata draft saved for an asset (asset editor's
/// "Preview as" switch).
#[tauri::command]
fn list_site_metadata(state: tauri::State<AppState>, asset_id: i64) -> Result<Vec<db::SiteMetadata>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::list_site_metadata(&conn, asset_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_site_metadata(
    state: tauri::State<AppState>,
    asset_id: i64,
    site_name: String,
    metadata: GeneratedMetadata,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::upsert_site_metadata(&conn, asset_id, &site_name, &metadata).map_err(|e| e.to_string())
}

/// Drop a site's draft so it falls back to the AI-generated default.
#[tauri::command]
fn delete_site_metadata(state: tauri::State<AppState>, asset_id: i64, site_name: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::delete_site_metadata(&conn, asset_id, &site_name).map_err(|e| e.to_string())
}

// --- metadata embedding (SPHIN-4) --------------------------------------------

#[tauri::command]
fn get_embed_config(
    state: tauri::State<AppState>,
    project_id: i64,
) -> Result<Option<EmbedConfig>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::get_embed_config(&conn, project_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_embed_config(
    state: tauri::State<AppState>,
    project_id: i64,
    config: EmbedConfig,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::set_embed_config(&conn, project_id, &config).map_err(|e| e.to_string())
}

fn metadata_from_record(record: &MetadataRecord) -> Result<GeneratedMetadata, String> {
    let keywords: Vec<String> =
        serde_json::from_str(&record.keywords_json).map_err(|e| e.to_string())?;
    Ok(GeneratedMetadata {
        title: record.title.clone(),
        description: record.description.clone(),
        keywords,
        profile: record.profile.clone(),
        meets_minimum_keywords: record.meets_minimum_keywords,
    })
}

/// Check whether exiftool is reachable at a project's configured path (or on
/// PATH), returning its version string. SPHIN-20.
#[tauri::command]
async fn check_exiftool(
    state: tauri::State<'_, AppState>,
    project_id: i64,
) -> Result<String, String> {
    let config = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::get_embed_config(&conn, project_id)
            .map_err(|e| e.to_string())?
            .unwrap_or_default()
    };
    tauri::async_runtime::spawn_blocking(move || embed::check_exiftool(&config))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Write the asset's latest generated metadata into the file itself as
/// IPTC/XMP via exiftool, and advance its status to `embedded`. SPHIN-4
/// (20). Runs on the blocking thread pool since it shells out to a
/// subprocess.
#[tauri::command]
async fn embed_asset_metadata(
    state: tauri::State<'_, AppState>,
    project_id: i64,
    asset_id: i64,
    site_name: Option<String>,
) -> Result<EmbedOutcome, String> {
    let (path, meta, config) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let asset = db::get_asset(&conn, asset_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("asset {asset_id} not found"))?;
        // A site's edited draft wins over the AI-generated default.
        let site_draft = match site_name.as_deref() {
            Some(site) => db::get_site_metadata(&conn, asset_id, site).map_err(|e| e.to_string())?,
            None => None,
        };
        let meta = match site_draft {
            Some(m) => m,
            None => {
                let record = db::latest_metadata_for_asset(&conn, asset_id)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "metadata has not been generated yet".to_string())?;
                metadata_from_record(&record)?
            }
        };
        let config = db::get_embed_config(&conn, project_id)
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        (asset.path, meta, config)
    };

    let outcome =
        tauri::async_runtime::spawn_blocking(move || embed::embed_metadata(&path, &meta, &config))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::set_asset_status(&conn, asset_id, "embedded").map_err(|e| e.to_string())?;
    Ok(outcome)
}

/// Export generated metadata for a set of assets to a CSV file at
/// `target_path`, for manual review or as a bulk-upload template for a
/// stock site (automated SFTP upload is SPHIN-6). SPHIN-21.
#[tauri::command]
fn export_metadata_csv(
    state: tauri::State<AppState>,
    asset_ids: Vec<i64>,
    target_path: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let mut assets = Vec::new();
    let mut metas = Vec::new();
    for id in &asset_ids {
        let asset = db::get_asset(&conn, *id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("asset {id} not found"))?;
        let record = db::latest_metadata_for_asset(&conn, *id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("asset {id} has no generated metadata"))?;
        metas.push(metadata_from_record(&record)?);
        assets.push(asset);
    }

    let rows: Vec<ExportRow> = assets
        .iter()
        .zip(metas.iter())
        .map(|(asset, meta)| ExportRow {
            file_name: asset.path.rsplit(['\\', '/']).next().unwrap_or(&asset.path),
            metadata: meta,
        })
        .collect();

    std::fs::write(&target_path, embed::export_csv(&rows)).map_err(|e| e.to_string())
}

// --- keyword enrichment (SPHIN-5) --------------------------------------------

const KEYWORD_CACHE_TTL_DAYS: i64 = 30;

fn non_empty(s: &str) -> Option<&str> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

#[tauri::command]
fn get_keyword_config(
    state: tauri::State<AppState>,
    project_id: i64,
) -> Result<Option<KeywordConfig>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    load_keyword_config(&conn, project_id)
}

#[tauri::command]
fn get_csv_layouts(state: tauri::State<AppState>, project_id: i64) -> Result<Option<String>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::get_csv_layouts(&conn, project_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_csv_layouts(state: tauri::State<AppState>, project_id: i64, json: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::set_csv_layouts(&conn, project_id, &json).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_keyword_config(
    state: tauri::State<AppState>,
    project_id: i64,
    mut config: KeywordConfig,
) -> Result<(), String> {
    for (purpose, creds) in keyword_slots(&mut config) {
        let name = api_key_name(project_id, purpose);
        match creds {
            Some(c) => stash_api_key(&name, &mut c.api_key)?,
            None => {
                let _ = secrets::delete_api_key(&name);
            }
        }
    }
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::set_keyword_config(&conn, project_id, &config).map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
struct EnrichResponse {
    record_id: i64,
    result: GeneratedMetadata,
    added: Vec<String>,
    errors: Vec<String>,
}

/// Enrich the asset's generated keywords with per-site suggestions from
/// whichever providers are configured (Shutterstock, Adobe Stock), seeded
/// from the asset's first generated keyword (or its title if it has none),
/// and respecting each site's rate limits via a local cache (SPHIN-24). A
/// single connector failing doesn't fail the whole call -- its error is
/// reported in `errors` and the other connector's suggestions still land.
/// SPHIN-5 (22/23/24). Runs the network calls on the blocking thread pool.
#[tauri::command]
async fn enrich_keywords(
    state: tauri::State<'_, AppState>,
    project_id: i64,
    asset_id: i64,
) -> Result<EnrichResponse, String> {
    let (record, config, existing_keywords, seed, limiter, mut collected) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let record = db::latest_metadata_for_asset(&conn, asset_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "metadata has not been generated yet".to_string())?;
        let config = load_keyword_config(&conn, project_id)?.unwrap_or_default();
        if config.shutterstock.is_none() && config.adobe_stock.is_none() {
            return Err("no keyword-enrichment provider configured for this project".to_string());
        }
        let existing_keywords = metadata_from_record(&record)?.keywords;
        let seed = existing_keywords
            .first()
            .cloned()
            .unwrap_or_else(|| record.title.clone());
        let limiter = db::get_limiter_profile(&conn, project_id)
            .map_err(|e| e.to_string())?
            .unwrap_or_default();

        let mut collected: Vec<(String, Vec<String>)> = Vec::new();
        if config.shutterstock.is_some() {
            if let Some(kw) =
                db::get_cached_keywords(&conn, &seed, "shutterstock", KEYWORD_CACHE_TTL_DAYS)
                    .map_err(|e| e.to_string())?
            {
                collected.push(("shutterstock".to_string(), kw));
            }
        }
        if config.adobe_stock.is_some() {
            if let Some(kw) =
                db::get_cached_keywords(&conn, &seed, "adobe_stock", KEYWORD_CACHE_TTL_DAYS)
                    .map_err(|e| e.to_string())?
            {
                collected.push(("adobe_stock".to_string(), kw));
            }
        }
        (record, config, existing_keywords, seed, limiter, collected)
    };

    let already_cached: std::collections::HashSet<String> =
        collected.iter().map(|(name, _)| name.clone()).collect();
    let seed_for_fetch = seed.clone();
    let config_for_fetch = config.clone();

    let (fetched, errors): (Vec<(String, Vec<String>)>, Vec<String>) =
        tauri::async_runtime::spawn_blocking(move || {
            let mut fetched = Vec::new();
            let mut errors = Vec::new();

            if let Some(creds) = &config_for_fetch.shutterstock {
                if !already_cached.contains("shutterstock") {
                    let outcome =
                        ShutterstockProvider::new(creds.api_key.clone(), non_empty(&creds.base_url))
                            .and_then(|p| p.suggest(&seed_for_fetch, 15));
                    match outcome {
                        Ok(kw) => fetched.push(("shutterstock".to_string(), kw)),
                        Err(e) => errors.push(format!("shutterstock: {e}")),
                    }
                }
            }
            if let Some(creds) = &config_for_fetch.adobe_stock {
                if !already_cached.contains("adobe_stock") {
                    let outcome =
                        AdobeStockProvider::new(creds.api_key.clone(), non_empty(&creds.base_url))
                            .and_then(|p| p.suggest(&seed_for_fetch, 15));
                    match outcome {
                        Ok(kw) => fetched.push(("adobe_stock".to_string(), kw)),
                        Err(e) => errors.push(format!("adobe_stock: {e}")),
                    }
                }
            }
            (fetched, errors)
        })
        .await
        .map_err(|e| e.to_string())?;

    collected.extend(fetched.iter().cloned());

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    for (provider, kw) in &fetched {
        let _ = db::set_cached_keywords(&conn, &seed, provider, kw);
    }

    let mut seen: std::collections::HashSet<String> =
        existing_keywords.iter().map(|k| k.to_lowercase()).collect();
    let mut merged = existing_keywords.clone();
    let mut added = Vec::new();
    'outer: for (_, kw_list) in &collected {
        for kw in kw_list {
            if merged.len() >= limiter.max_keywords {
                break 'outer;
            }
            let lower = kw.to_lowercase();
            if lower.is_empty() || lower.chars().count() > limiter.max_keyword_chars {
                continue;
            }
            if seen.insert(lower) {
                merged.push(kw.clone());
                added.push(kw.clone());
            }
        }
    }

    let meets_minimum_keywords = merged.len() >= limiter.min_keywords;
    let enriched = GeneratedMetadata {
        title: record.title.clone(),
        description: record.description.clone(),
        keywords: merged,
        profile: record.profile.clone(),
        meets_minimum_keywords,
    };
    let saved = db::insert_metadata(&conn, asset_id, &enriched).map_err(|e| e.to_string())?;

    Ok(EnrichResponse {
        record_id: saved.id,
        result: enriched,
        added,
        errors,
    })
}

// --- job orchestration (SPHIN-7) ---------------------------------------------

const QUEUE_JOB_TYPES: [&str; 5] = ["analyze", "generate_metadata", "embed", "enrich_keywords", "upload"];
const QUEUE_POLL_INTERVAL_MS: u64 = 400;
/// Safety-net ceiling on a single claimed job, on top of any timeout its own
/// implementation sets internally (e.g. SFTP's connect/IO timeouts). Without
/// this, a job whose underlying call hangs with no timeout of its own would
/// block the single worker -- and every job queued behind it -- forever.
const JOB_TIMEOUT_SECS: u64 = 180;
/// After this many automatic retries a job is left `failed` for manual
/// retry rather than being rescheduled again. SPHIN-26.
const MAX_AUTO_RETRIES: i64 = 3;
/// Delay before each automatic retry, indexed by the job's attempt count
/// (clamped to the last entry once exhausted).
const BACKOFF_SCHEDULE_SECS: [i64; 3] = [10, 60, 300];

/// Enqueue one batch job per asset for the background worker to pick up.
/// `job_type` is one of [`QUEUE_JOB_TYPES`]. SPHIN-28.
#[tauri::command]
fn enqueue_batch(
    state: tauri::State<AppState>,
    project_id: i64,
    asset_ids: Vec<i64>,
    job_type: String,
    profile_id: Option<i64>,
    site_name: Option<String>,
) -> Result<Vec<Job>, String> {
    if !QUEUE_JOB_TYPES.contains(&job_type.as_str()) {
        return Err(format!("unknown job type: {job_type}"));
    }
    if job_type == "upload" && profile_id.is_none() {
        return Err("upload jobs require an SFTP profile_id".to_string());
    }
    let payload =
        serde_json::json!({ "project_id": project_id, "profile_id": profile_id, "site_name": site_name }).to_string();
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::enqueue_jobs(&conn, &asset_ids, &job_type, &payload).map_err(|e| e.to_string())
}

/// Batch (queue) jobs, most recent first, for the progress dashboard. SPHIN-29.
#[tauri::command]
fn list_queue_jobs(state: tauri::State<AppState>, limit: i64) -> Result<Vec<Job>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::list_queue_jobs(&conn, limit).map_err(|e| e.to_string())
}

#[tauri::command]
fn queue_job_counts(state: tauri::State<AppState>) -> Result<JobCounts, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::queue_job_counts(&conn).map_err(|e| e.to_string())
}

/// Re-enqueue a failed job for another attempt. SPHIN-30.
#[tauri::command]
fn retry_job(state: tauri::State<AppState>, job_id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::retry_job(&conn, job_id).map_err(|e| e.to_string())
}

/// Cancel a queue job. If it's still `pending`, it's marked `cancelled`
/// directly and the worker will never claim it. If it's the job the worker
/// is currently running, wake the worker's cancellation signal instead --
/// see [`spawn_job_worker`]'s `notify.notified()` branch. A job that's
/// already `done`/`failed`/`cancelled` is a silent no-op.
#[tauri::command]
fn cancel_job(state: tauri::State<AppState>, job_id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    if db::cancel_pending_job(&conn, job_id).map_err(|e| e.to_string())? {
        return Ok(());
    }
    drop(conn);
    if let Ok(running) = state.running_job.lock() {
        if let Some((id, notify)) = running.as_ref() {
            if *id == job_id {
                notify.notify_one();
            }
        }
    }
    Ok(())
}

/// Remove finished queue jobs from Activity: the given ids, or (with no ids)
/// every finished job with `status`. Pending/running jobs are left alone.
#[tauri::command]
fn remove_jobs(
    state: tauri::State<AppState>,
    job_ids: Option<Vec<i64>>,
    status: Option<String>,
) -> Result<usize, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::delete_finished_jobs(&conn, job_ids.as_deref(), status.as_deref()).map_err(|e| e.to_string())
}

/// The full status-change history for one job (queued, started, retried,
/// done/failed), each with its own timestamp -- what Activity's per-job log
/// renders, replacing a synthesized 2-3 line summary with what actually
/// happened.
#[tauri::command]
fn list_job_events(state: tauri::State<AppState>, job_id: i64) -> Result<Vec<JobEvent>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::list_job_events(&conn, job_id).map_err(|e| e.to_string())
}

#[derive(Debug, serde::Deserialize)]
struct QueueJobPayload {
    #[serde(default = "default_project_id")]
    project_id: i64,
    #[serde(default)]
    profile_id: Option<i64>,
    /// Target site whose edited metadata draft an `embed` job should write.
    #[serde(default)]
    site_name: Option<String>,
}

fn default_project_id() -> i64 {
    1
}

/// Run one claimed job by delegating to the same command handlers a direct
/// single-asset click would use -- the worker is just another caller of
/// them, so there is exactly one implementation of each operation to keep
/// correct.
async fn execute_queue_job(app: &tauri::AppHandle, job: &Job) -> Result<(), String> {
    let payload: QueueJobPayload = serde_json::from_str(&job.payload_json).unwrap_or(QueueJobPayload {
        project_id: 1,
        profile_id: None,
        site_name: None,
    });
    let state: tauri::State<'_, AppState> = app.state();

    match job.job_type.as_str() {
        "analyze" => analyze_asset(state, payload.project_id, job.asset_id)
            .await
            .map(|_| ()),
        "generate_metadata" => generate_metadata(state, payload.project_id, job.asset_id).map(|_| ()),
        "embed" => embed_asset_metadata(state, payload.project_id, job.asset_id, payload.site_name.clone())
            .await
            .map(|_| ()),
        "enrich_keywords" => enrich_keywords(state, payload.project_id, job.asset_id)
            .await
            .map(|_| ()),
        "upload" => {
            let profile_id = payload
                .profile_id
                .ok_or_else(|| "upload job is missing a profile_id".to_string())?;
            upload_asset(state, profile_id, job.asset_id).await
        }
        other => Err(format!("unknown job type: {other}")),
    }
}

/// The single background worker: claim the oldest pending queue job, run it,
/// record the outcome, and push a `job-updated` event so the dashboard can
/// update live -- Tauri's IPC event bridge stands in for the WebSocket
/// called out in SPHIN-29, since frontend and backend already share a
/// process and don't need an actual socket between them. One job at a time
/// keeps this simple and naturally respects external rate limits (SPHIN-24)
/// without extra coordination.
/// How a claimed job's execution wound down: either it ran to completion (or
/// timed out, which looks the same as any other failure downstream), or the
/// user cancelled it via [`cancel_job`] while it was running.
enum JobOutcome {
    Finished(Result<(), String>),
    Cancelled,
}

fn spawn_job_worker(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let claimed = {
                let state: tauri::State<'_, AppState> = app.state();
                let conn = match state.db.lock() {
                    Ok(c) => c,
                    Err(_) => break,
                };
                db::claim_next_pending_job(&conn).ok().flatten()
            };

            let Some(job) = claimed else {
                tokio::time::sleep(std::time::Duration::from_millis(QUEUE_POLL_INTERVAL_MS)).await;
                continue;
            };

            let _ = app.emit("job-updated", &job);

            // Tracked so `cancel_job` can wake `notify.notified()` below for
            // a job that's already running (a still-pending job is cancelled
            // directly in the DB instead, without needing this).
            let notify = Arc::new(tokio::sync::Notify::new());
            {
                let state: tauri::State<'_, AppState> = app.state();
                let mut running = state.running_job.lock().unwrap();
                *running = Some((job.id, notify.clone()));
            }

            let outcome = tokio::select! {
                r = execute_queue_job(&app, &job) => JobOutcome::Finished(r),
                _ = tokio::time::sleep(std::time::Duration::from_secs(JOB_TIMEOUT_SECS)) => {
                    JobOutcome::Finished(Err(format!(
                        "{} job timed out after {}s with no response",
                        job.job_type, JOB_TIMEOUT_SECS
                    )))
                }
                _ = notify.notified() => JobOutcome::Cancelled,
            };

            {
                let state: tauri::State<'_, AppState> = app.state();
                let mut running = state.running_job.lock().unwrap();
                *running = None;
            }

            let updated = match outcome {
                JobOutcome::Cancelled => {
                    let state: tauri::State<'_, AppState> = app.state();
                    let conn = match state.db.lock() {
                        Ok(c) => c,
                        Err(_) => break,
                    };
                    let _ = db::set_job_status(&conn, job.id, "cancelled", Some("Cancelled by user"));
                    Job {
                        status: "cancelled".to_string(),
                        error: Some("Cancelled by user".to_string()),
                        ..job
                    }
                }
                JobOutcome::Finished(Ok(())) => {
                    let state: tauri::State<'_, AppState> = app.state();
                    let conn = match state.db.lock() {
                        Ok(c) => c,
                        Err(_) => break,
                    };
                    let _ = db::set_job_status(&conn, job.id, "done", None);
                    Job {
                        status: "done".to_string(),
                        error: None,
                        ..job
                    }
                }
                JobOutcome::Finished(Err(e)) => {
                    if job.attempts < MAX_AUTO_RETRIES {
                        let delay = BACKOFF_SCHEDULE_SECS
                            [(job.attempts as usize).min(BACKOFF_SCHEDULE_SECS.len() - 1)];
                        let state: tauri::State<'_, AppState> = app.state();
                        let conn = match state.db.lock() {
                            Ok(c) => c,
                            Err(_) => break,
                        };
                        let _ = db::schedule_retry(&conn, job.id, delay, &e);
                        Job {
                            status: "pending".to_string(),
                            error: Some(e.clone()),
                            attempts: job.attempts + 1,
                            ..job
                        }
                    } else {
                        let state: tauri::State<'_, AppState> = app.state();
                        let conn = match state.db.lock() {
                            Ok(c) => c,
                            Err(_) => break,
                        };
                        let _ = db::set_job_status(&conn, job.id, "failed", Some(&e));
                        Job {
                            status: "failed".to_string(),
                            error: Some(e.clone()),
                            ..job
                        }
                    }
                }
            };
            let _ = app.emit("job-updated", &updated);
        }
    });
}

// --- SFTP upload (SPHIN-6) ----------------------------------------------------

fn site_to_str(site: &str) -> SftpSite {
    match site {
        "adobe_stock" => SftpSite::AdobeStock,
        _ => SftpSite::Generic,
    }
}

fn protocol_to_str(protocol: &str) -> TransportProtocol {
    match protocol {
        "ftps" => TransportProtocol::Ftps,
        _ => TransportProtocol::Sftp,
    }
}

fn sftp_profile_from_record(r: &SftpProfileRecord) -> SftpProfile {
    SftpProfile {
        id: r.id,
        name: r.name.clone(),
        site: site_to_str(&r.site),
        protocol: protocol_to_str(&r.protocol),
        host: r.host.clone(),
        port: r.port as u16,
        username: r.username.clone(),
        remote_dir: r.remote_dir.clone(),
        credential_key: r.credential_key.clone(),
        host_key_fingerprint: r.host_key_fingerprint.clone(),
    }
}

#[tauri::command]
fn list_sftp_profiles(state: tauri::State<AppState>, project_id: i64) -> Result<Vec<SftpProfileRecord>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::list_sftp_profiles(&conn, project_id).map_err(|e| e.to_string())
}

/// Create a connection profile and save its password (if given) in the OS
/// credential store. SPHIN-25.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn create_sftp_profile(
    state: tauri::State<AppState>,
    project_id: i64,
    name: String,
    site: String,
    protocol: String,
    host: String,
    port: i64,
    username: String,
    remote_dir: String,
    password: String,
) -> Result<SftpProfileRecord, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let profile = db::create_sftp_profile(&conn, project_id, &name, &site, &protocol, &host, port, &username, &remote_dir)
        .map_err(|e| e.to_string())?;
    if !password.is_empty() {
        secrets::set_secret(&profile.credential_key, &password).map_err(|e| e.to_string())?;
    }
    Ok(profile)
}

/// Update a profile's connection details; `password`, if given, replaces
/// the saved credential (an empty/omitted password leaves it untouched).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
fn update_sftp_profile(
    state: tauri::State<AppState>,
    id: i64,
    name: String,
    site: String,
    protocol: String,
    host: String,
    port: i64,
    username: String,
    remote_dir: String,
    password: Option<String>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::update_sftp_profile(&conn, id, &name, &site, &protocol, &host, port, &username, &remote_dir)
        .map_err(|e| e.to_string())?;
    if let Some(pw) = password {
        if !pw.is_empty() {
            let profile = db::get_sftp_profile(&conn, id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("SFTP profile {id} not found"))?;
            secrets::set_secret(&profile.credential_key, &pw).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
fn delete_sftp_profile(state: tauri::State<AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    if let Some(profile) = db::get_sftp_profile(&conn, id).map_err(|e| e.to_string())? {
        let _ = secrets::delete_secret(&profile.credential_key);
    }
    db::delete_sftp_profile(&conn, id).map_err(|e| e.to_string())
}

/// Upload one asset's file to an SFTP profile's remote directory, pinning
/// the host key on first connect (or verifying it on later ones). Runs on
/// the blocking thread pool since the transport is a blocking call
/// internally. SPHIN-6 (25/26/27).
#[tauri::command]
async fn upload_asset(
    state: tauri::State<'_, AppState>,
    profile_id: i64,
    asset_id: i64,
) -> Result<(), String> {
    let (record, password, asset) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let record = db::get_sftp_profile(&conn, profile_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("SFTP profile {profile_id} not found"))?;
        let password = secrets::get_secret(&record.credential_key)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no password saved for this SFTP profile".to_string())?;
        let asset = db::get_asset(&conn, asset_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("asset {asset_id} not found"))?;
        (record, password, asset)
    };

    let profile = sftp_profile_from_record(&record);
    let filename = asset
        .path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(&asset.path)
        .to_string();
    let local_path = std::path::PathBuf::from(&asset.path);

    let outcome = tauri::async_runtime::spawn_blocking(move || {
        upload::upload_file(&profile, &password, &local_path, &filename)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    if record.host_key_fingerprint.as_deref() != Some(outcome.host_key_fingerprint.as_str()) {
        let _ = db::set_sftp_host_key_fingerprint(&conn, record.id, &outcome.host_key_fingerprint);
    }
    db::set_asset_status(&conn, asset_id, "uploaded").map_err(|e| e.to_string())?;
    Ok(())
}

/// Connect and authenticate against a saved SFTP profile without
/// transferring a file, so credentials/reachability can be checked before
/// (or without) a real upload. Pins the host key the same way a real upload
/// does, so a successful test also satisfies trust-on-first-use.
#[tauri::command]
async fn test_sftp_connection(state: tauri::State<'_, AppState>, profile_id: i64) -> Result<String, String> {
    let (record, password) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let record = db::get_sftp_profile(&conn, profile_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("SFTP profile {profile_id} not found"))?;
        let password = secrets::get_secret(&record.credential_key)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no password saved for this SFTP profile".to_string())?;
        (record, password)
    };

    let profile = sftp_profile_from_record(&record);
    let outcome = tauri::async_runtime::spawn_blocking(move || upload::test_connection(&profile, &password))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    let conn = state.db.lock().map_err(|e| e.to_string())?;
    if record.host_key_fingerprint.as_deref() != Some(outcome.host_key_fingerprint.as_str()) {
        let _ = db::set_sftp_host_key_fingerprint(&conn, record.id, &outcome.host_key_fingerprint);
    }
    Ok(outcome.host_key_fingerprint)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("sphinx.db");
            let conn = db::open(db_path)?;
            migrate_plaintext_api_keys(&conn);
            app.manage(AppState {
                db: Mutex::new(conn),
                watch: Mutex::new(None),
                running_job: Mutex::new(None),
            });
            spawn_job_worker(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ingest_files,
            ingest_folder,
            start_watch,
            stop_watch,
            list_assets,
            asset_count,
            delete_asset,
            list_projects,
            create_project,
            get_analysis_config,
            set_analysis_config,
            get_analysis_configs,
            check_ollama,
            detect_gpu,
            get_analysis,
            analyze_asset,
            get_video_config,
            set_video_config,
            check_ffmpeg,
            get_transcription_config,
            set_transcription_config,
            list_limiter_profiles,
            get_limiter_profile,
            set_limiter_profile,
            list_site_profiles,
            add_site_profile,
            remove_site_profile,
            get_metadata,
            generate_metadata,
            set_metadata,
            list_site_metadata,
            set_site_metadata,
            delete_site_metadata,
            get_embed_config,
            set_embed_config,
            check_exiftool,
            embed_asset_metadata,
            export_metadata_csv,
            get_keyword_config,
            set_keyword_config,
            get_csv_layouts,
            set_csv_layouts,
            enrich_keywords,
            enqueue_batch,
            list_queue_jobs,
            queue_job_counts,
            retry_job,
            cancel_job,
            remove_jobs,
            list_job_events,
            list_sftp_profiles,
            create_sftp_profile,
            update_sftp_profile,
            delete_sftp_profile,
            upload_asset,
            test_sftp_connection,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
