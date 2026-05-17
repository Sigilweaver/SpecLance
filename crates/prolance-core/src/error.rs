use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("lance error: {0}")]
    Lance(#[from] lancedb::Error),

    #[error("arrow error: {0}")]
    Arrow(#[from] arrow_schema::ArrowError),

    #[error("serde_json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("schema mismatch: {0}")]
    Schema(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Error::Other(e.to_string())
    }
}
