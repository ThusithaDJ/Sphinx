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

/// A row in the `jobs` table: one unit of async work against an asset
/// (analysis, metadata generation, embedding, upload, ...).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: i64,
    pub asset_id: i64,
    pub job_type: String,
    pub status: String,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Outcome of attempting to ingest a single path.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum IngestOutcome {
    Ingested { asset: Asset },
    Duplicate { existing: Asset, path: String },
    Skipped { path: String, reason: String },
}
