// Writer stub - to be filled in next commit.
use prolance_core::{Chromatogram, Run, Spectrum};
use std::io::Write;

use crate::error::MsResult;

/// Write an mzML document.
pub fn write_mzml<W: Write>(
    _out: &mut W,
    _run: &Run,
    _spectra: &[Spectrum],
    _chromatograms: &[Chromatogram],
) -> MsResult<()> {
    Err(crate::error::MsError::Unsupported(
        "mzml writer not yet implemented".into(),
    ))
}
