use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("watch error: {0}")]
    Watch(String),
    #[error("path is not a file: {0}")]
    NotAFile(String),
    #[error("unsupported media type for: {0}")]
    UnsupportedMedia(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
