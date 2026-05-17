// Reader stub - to be filled in next commit.
use prolance_core::{Chromatogram, Run, Spectrum};
use std::path::Path;

use crate::error::MsResult;

/// Container returned by [`read_mzml`].
#[derive(Debug, Default)]
pub struct MzmlData {
    pub run: Run,
    pub spectra: Vec<Spectrum>,
    pub chromatograms: Vec<Chromatogram>,
}

/// Parse an mzML file into a [`MzmlData`] bundle.
pub fn read_mzml<P: AsRef<Path>>(_path: P) -> MsResult<MzmlData> {
    Err(crate::error::MsError::Unsupported(
        "mzml reader not yet implemented".into(),
    ))
}
