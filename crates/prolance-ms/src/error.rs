use thiserror::Error;

#[derive(Debug, Error)]
pub enum MsError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("core error: {0}")]
    Core(#[from] prolance_core::Error),

    #[error("xml error: {0}")]
    Xml(String),

    #[error("base64 error: {0}")]
    Base64(String),

    #[error("malformed mzML: {0}")]
    Malformed(String),

    #[error("unsupported: {0}")]
    Unsupported(String),

    #[error("{0}")]
    Other(String),
}

pub type MsResult<T> = std::result::Result<T, MsError>;

impl From<quick_xml::Error> for MsError {
    fn from(e: quick_xml::Error) -> Self {
        MsError::Xml(e.to_string())
    }
}

impl From<quick_xml::events::attributes::AttrError> for MsError {
    fn from(e: quick_xml::events::attributes::AttrError) -> Self {
        MsError::Xml(e.to_string())
    }
}

impl From<base64::DecodeError> for MsError {
    fn from(e: base64::DecodeError) -> Self {
        MsError::Base64(e.to_string())
    }
}

impl From<serde_json::Error> for MsError {
    fn from(e: serde_json::Error) -> Self {
        MsError::Other(e.to_string())
    }
}
