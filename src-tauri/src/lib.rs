// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Serialize;
use sphinx_core::analysis::{AnalysisConfig, AnalysisResult};
use sphinx_core::embed::{EmbedConfig, EmbedOutcome, ExportRow};
use sphinx_core::metadata::{GeneratedMetadata, LimiterProfile};
use sphinx_core::models::{AnalysisRecord, Asset, IngestOutcome, MetadataRecord, Project};
use sphinx_core::watch::WatchHandle;
use sphinx_core::{analysis, db, embed, ingest, metadata};
use tauri::{Emitter, Manager};

/// Shared app state: the SQLite connection and the current folder-watch
/// handle (if any). A single connection behind a mutex is plenty for the
/// ingestion workload here (no long-running writes) and keeps SPHIN-1 simple;
/// a connection pool can replace this later if job orchestration (SPHIN-7)
/// needs concurrent writers.
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

/// Run the project's configured vision model against one image asset, store the
/// structured result, and advance the asset to `analyzed`. SPHIN-2 (15/16/17).
///
/// The network call is blocking, so it runs on the blocking thread pool; the
/// DB mutex is only held to read inputs and write results, never across the
/// call itself.
#[tauri::command]
async fn analyze_asset(
    state: tauri::State<'_, AppState>,
    project_id: i64,
    asset_id: i64,
) -> Result<AnalysisResponse, String> {
    let (config, asset) = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        let config = db::get_analysis_config(&conn, project_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "no analysis provider configured for this project".to_string())?;
        let asset = db::get_asset(&conn, asset_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("asset {asset_id} not found"))?;
        (config, asset)
    };

    if asset.media_type != "image" {
        return Err("analysis currently supports images only (video is SPHIN-8)".into());
    }

    let job = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        db::insert_job(&conn, asset_id, "analysis").map_err(|e| e.to_string())?
    };

    let path = asset.path.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || analysis::analyze_file(&config, &path))
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
            get_analysis,
            analyze_asset,
            list_limiter_profiles,
            get_limiter_profile,
            set_limiter_profile,
            get_metadata,
            generate_metadata,
            get_embed_config,
            set_embed_config,
            check_exiftool,
            embed_asset_metadata,
            export_metadata_csv,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
