//! Thermo Fisher `.raw` ingest adapter (built on top of [`opentfraw`]).
//!
//! This adapter is a stub - to be filled in once the mzML codec is verified.

use std::path::Path;

use crate::error::{MsError, MsResult};
use crate::mzml::MzmlData;

/// Ingest a Thermo `.raw` file into ProLance records.
///
/// Internally this converts via mzML using `opentfraw`'s writer and then
/// re-parses with our own mzML reader. This keeps the ingest path
/// uniform with mzML/Bruker/Waters and ensures the same correctness
/// guarantees.
pub fn ingest<P: AsRef<Path>>(_path: P) -> MsResult<MzmlData> {
    Err(MsError::Unsupported(
        "thermo adapter not yet implemented".into(),
    ))
}
