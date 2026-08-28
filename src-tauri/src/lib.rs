// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Serialize;
use sphinx_core::analysis::{AnalysisConfig, AnalysisResult};
use sphinx_core::db::JobCounts;
use sphinx_core::embed::{EmbedConfig, EmbedOutcome, ExportRow};
use sphinx_core::keywords::{AdobeStockProvider, KeywordConfig, KeywordProvider, ShutterstockProvider};
use sphinx_core::metadata::{GeneratedMetadata, LimiterProfile};
use sphinx_core::models::{AnalysisRecord, Asset, IngestOutcome, Job, MetadataRecord, Project, SftpProfileRecord};
use sphinx_core::secrets;
use sphinx_core::transcribe::TranscriptionConfig;
use sphinx_core::upload::{SftpProfile, SftpSite};
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
#[tauri::command]
fn ingest_files(state: tauri::State<AppState>, paths: Vec<String>) -> Result<IngestSummary, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let results = ingest::ingest_paths(&conn, paths);
    Ok(summarize(results))
}

/// Recursively ingest every supported file in a folder (used both for a
/// one-off "import this folder" action and to backfill when watch mode is
/// first turned on for a folder). SPHIN-11 / SPHIN-12.
#[tauri::command]
fn ingest_folder(state: tauri::State<AppState>, dir: String) -> Result<IngestSummary, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let results = ingest::ingest_directory(&conn, &dir);
    Ok(summarize(results))
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
    db::get_analysis_config(&conn, project_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_analysis_config(
    state: tauri::State<AppState>,
    project_id: i64,
    config: AnalysisConfig,
) -> Result<(), String> {
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
    db::get_analysis_configs(&conn, project_id).map_err(|e| e.to_string())
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
        let config = db::get_analysis_config(&conn, project_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no analysis provider configured for this project".to_string())?;
        let asset = db::get_asset(&conn, asset_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("asset {asset_id} not found"))?;
        let video_config = db::get_video_config(&conn, project_id)
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let transcription_config = db::get_transcription_config(&conn, project_id)
            .map_err(|e| e.to_string())?;
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
    db::get_transcription_config(&conn, project_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_transcription_config(
    state: tauri::State<AppState>,
    project_id: i64,
    config: TranscriptionConfig,
) -> Result<(), String> {
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
) -> Result<EmbedOutcome, String> {
    let (path, meta, config) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let asset = db::get_asset(&conn, asset_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("asset {asset_id} not found"))?;
        let record = db::latest_metadata_for_asset(&conn, asset_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "metadata has not been generated yet".to_string())?;
        let meta = metadata_from_record(&record)?;
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
    db::get_keyword_config(&conn, project_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_keyword_config(
    state: tauri::State<AppState>,
    project_id: i64,
    config: KeywordConfig,
) -> Result<(), String> {
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
        let config = db::get_keyword_config(&conn, project_id)
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
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
) -> Result<Vec<Job>, String> {
    if !QUEUE_JOB_TYPES.contains(&job_type.as_str()) {
        return Err(format!("unknown job type: {job_type}"));
    }
    if job_type == "upload" && profile_id.is_none() {
        return Err("upload jobs require an SFTP profile_id".to_string());
    }
    let payload = serde_json::json!({ "project_id": project_id, "profile_id": profile_id }).to_string();
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

#[derive(Debug, serde::Deserialize)]
struct QueueJobPayload {
    #[serde(default = "default_project_id")]
    project_id: i64,
    #[serde(default)]
    profile_id: Option<i64>,
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
    });
    let state: tauri::State<'_, AppState> = app.state();

    match job.job_type.as_str() {
        "analyze" => analyze_asset(state, payload.project_id, job.asset_id)
            .await
            .map(|_| ()),
        "generate_metadata" => generate_metadata(state, payload.project_id, job.asset_id).map(|_| ()),
        "embed" => embed_asset_metadata(state, payload.project_id, job.asset_id)
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

            let result = execute_queue_job(&app, &job).await;

            let updated = match &result {
                Ok(()) => {
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
                Err(e) => {
                    if job.attempts < MAX_AUTO_RETRIES {
                        let delay = BACKOFF_SCHEDULE_SECS
                            [(job.attempts as usize).min(BACKOFF_SCHEDULE_SECS.len() - 1)];
                        let state: tauri::State<'_, AppState> = app.state();
                        let conn = match state.db.lock() {
                            Ok(c) => c,
                            Err(_) => break,
                        };
                        let _ = db::schedule_retry(&conn, job.id, delay, e);
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
                        let _ = db::set_job_status(&conn, job.id, "failed", Some(e));
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

fn sftp_profile_from_record(r: &SftpProfileRecord) -> SftpProfile {
    SftpProfile {
        id: r.id,
        name: r.name.clone(),
        site: site_to_str(&r.site),
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
fn create_sftp_profile(
    state: tauri::State<AppState>,
    project_id: i64,
    name: String,
    site: String,
    host: String,
    port: i64,
    username: String,
    remote_dir: String,
    password: String,
) -> Result<SftpProfileRecord, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let profile = db::create_sftp_profile(&conn, project_id, &name, &site, &host, port, &username, &remote_dir)
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
    host: String,
    port: i64,
    username: String,
    remote_dir: String,
    password: Option<String>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    db::update_sftp_profile(&conn, id, &name, &site, &host, port, &username, &remote_dir)
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
            app.manage(AppState {
                db: Mutex::new(conn),
                watch: Mutex::new(None),
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
            list_projects,
            create_project,
            get_analysis_config,
            set_analysis_config,
            get_analysis_configs,
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
            get_metadata,
            generate_metadata,
            set_metadata,
            get_embed_config,
            set_embed_config,
            check_exiftool,
            embed_asset_metadata,
            export_metadata_csv,
            get_keyword_config,
            set_keyword_config,
            enrich_keywords,
            enqueue_batch,
            list_queue_jobs,
            queue_job_counts,
            retry_job,
            list_sftp_profiles,
            create_sftp_profile,
            update_sftp_profile,
            delete_sftp_profile,
            upload_asset,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
