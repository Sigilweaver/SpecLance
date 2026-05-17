//! Bruker timsTOF `.d/` ingest adapter (built on top of [`opentimstdf`]).
//!
//! This adapter is a stub - to be filled in once the mzML codec is verified.

use std::path::Path;

use crate::error::{MsError, MsResult};
use crate::mzml::MzmlData;

/// Ingest a Bruker timsTOF `.d/` bundle into ProLance records.
pub fn ingest<P: AsRef<Path>>(_path: P) -> MsResult<MzmlData> {
    Err(MsError::Unsupported(
        "bruker adapter not yet implemented".into(),
    ))
}
