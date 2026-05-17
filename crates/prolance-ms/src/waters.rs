//! Waters MassLynx `.raw` ingest adapter (built on top of [`openwraw`]).
//!
//! This adapter is a stub - to be filled in once the mzML codec is verified.

use std::path::Path;

use crate::error::{MsError, MsResult};
use crate::mzml::MzmlData;

/// Ingest a Waters MassLynx `.raw` directory into ProLance records.
pub fn ingest<P: AsRef<Path>>(_path: P) -> MsResult<MzmlData> {
    Err(MsError::Unsupported(
        "waters adapter not yet implemented".into(),
    ))
}
