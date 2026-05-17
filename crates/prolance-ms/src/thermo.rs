//! Thermo Fisher `.raw` ingest adapter.
//!
//! Routes through [`opentfraw`]'s mzML writer into our in-house mzML
//! reader so the rest of the ProLance pipeline stays uniform.

use std::fs::File;
use std::path::Path;

use opentfraw::{write_mzml, RawFileReader};

use crate::error::{MsError, MsResult};
use crate::mzml::{parse_bytes, MzmlData};

/// Ingest a Thermo `.raw` file into ProLance records.
///
/// Internally this converts via mzML using `opentfraw`'s writer and
/// then re-parses with our own mzML reader. This keeps the ingest
/// path identical to the native mzML one.
pub fn ingest<P: AsRef<Path>>(path: P) -> MsResult<MzmlData> {
    let path = path.as_ref();
    let raw = RawFileReader::open_path(path)
        .map_err(|e| MsError::Other(format!("opentfraw open: {e}")))?;
    let mut source = File::open(path).map_err(MsError::Io)?;
    let raw_filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("source.raw");

    let mut buf: Vec<u8> = Vec::with_capacity(1 << 20);
    write_mzml(&raw, &mut source, &mut buf, raw_filename, false)
        .map_err(|e| MsError::Other(format!("opentfraw write_mzml: {e}")))?;

    parse_bytes(&buf, path.to_string_lossy().to_string())
}
