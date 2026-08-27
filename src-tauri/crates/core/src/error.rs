use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("watch error: {0}")]
    Watch(String),
    #[error("path is not a file: {0}")]
    NotAFile(String),
    #[error("unsupported media type for: {0}")]
    UnsupportedMedia(String),
    // --- SPHIN-2: media analysis ---
    #[error("analysis is not configured: {0}")]
    Config(String),
    #[error("http error talking to {provider}: {source}")]
    Http {
        provider: String,
        #[source]
        source: reqwest::Error,
    },
    /// The provider returned a non-2xx response; `message` is a trimmed excerpt
    /// of its error body (useful for surfacing "invalid api key" etc. to the UI).
    #[error("{provider} returned {status}: {message}")]
    Provider {
        provider: String,
        status: u16,
        message: String,
    },
    /// The provider replied 2xx but the body wasn't the shape we expected
    /// (missing fields, non-JSON model output, refusal, ...).
    #[error("could not parse {provider} response: {message}")]
    BadResponse { provider: String, message: String },
    // --- SPHIN-4: metadata embedding ---
    /// An external CLI tool (exiftool) couldn't be run, or exited non-zero.
    #[error("{tool} error: {message}")]
    ExternalTool { tool: String, message: String },
}

pub type Result<T> = std::result::Result<T, CoreError>;
