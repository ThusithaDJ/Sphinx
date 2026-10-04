use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaType {
    Image,
    Video,
}

impl MediaType {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" | "png" | "tif" | "tiff" | "webp" | "bmp" | "heic" | "eps" | "svg" => {
                Some(MediaType::Image)
            }
            "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" => Some(MediaType::Video),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            MediaType::Image => "image",
            MediaType::Video => "video",
        }
    }
}

/// A row in the `assets` table: one ingested source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: i64,
    pub path: String,
    pub hash: String,
    pub size: i64,
    pub media_type: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A row in the `jobs` table: one unit of work against an asset (analysis,
/// metadata generation, embedding, keyword enrichment, ...).
///
/// `source` distinguishes two independent lifecycles sharing this table:
/// `"direct"` rows are a fire-and-forget history log written by a
/// synchronous single-asset command (already `done`/`failed` by the time
/// they're visible) and `"queue"` rows are batch jobs (SPHIN-7) that start
/// `pending` and are claimed and driven through `running` -> `done`/`failed`
/// by the background worker. The worker only ever claims `"queue"` rows, so
/// the two lifecycles never race on the same row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: i64,
    pub asset_id: i64,
    pub job_type: String,
    pub status: String,
    pub error: Option<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default = "default_job_payload")]
    pub payload_json: String,
    #[serde(default)]
    pub attempts: i64,
    /// A queue job claimable only once this timestamp (RFC3339) has passed;
    /// `None` means claimable immediately. Used for exponential backoff on
    /// automatic retry (SPHIN-26).
    #[serde(default)]
    pub not_before: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn default_job_payload() -> String {
    "{}".to_string()
}

/// One recorded status transition for a job -- queued, started, retried,
/// done/failed -- each with its own timestamp and message, so Activity can
/// show the real history a job went through instead of only its current
/// `status`/`error` (which each new transition overwrites on `jobs` itself).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobEvent {
    pub id: i64,
    pub job_id: i64,
    pub status: String,
    pub message: String,
    pub at: String,
}

/// A row in the `sftp_profiles` table: a named SFTP connection profile for
/// one stock site (SPHIN-25). The password lives in the OS credential store
/// under `credential_key`, never here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpProfileRecord {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub site: String,
    pub protocol: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub remote_dir: String,
    pub credential_key: String,
    pub host_key_fingerprint: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A row in the `projects` table. A project is a working set of assets that
/// share configuration (AI provider, limiter profile, upload targets). Every
/// database has a "Default" project (id 1); more can be created later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A row in the `analyses` table: one stored vision-model result for an asset.
/// `result_json` is the serialized [`crate::analysis::AnalysisResult`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRecord {
    pub id: i64,
    pub asset_id: i64,
    pub provider: String,
    pub model: String,
    pub result_json: String,
    pub created_at: String,
}

/// A row in the `metadata` table: one generated title/description/keyword
/// set for an asset (SPHIN-3). `keywords_json` is a serialized `Vec<String>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataRecord {
    pub id: i64,
    pub asset_id: i64,
    pub title: String,
    pub description: String,
    pub keywords_json: String,
    pub profile: String,
    pub meets_minimum_keywords: bool,
    pub created_at: String,
}

/// Outcome of attempting to ingest a single path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum IngestOutcome {
    Ingested { asset: Asset },
    Duplicate { existing: Asset, path: String },
    Skipped { path: String, reason: String },
}
