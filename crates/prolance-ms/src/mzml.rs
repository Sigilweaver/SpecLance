//! mzML reader and writer.
//!
//! The reader produces [`prolance_core::Spectrum`] and [`Chromatogram`]
//! records and a [`Run`] header describing the file. The writer takes
//! the same records and emits a spec-valid mzML 1.1 document.
//!
//! Roundtrip guarantee: `read_mzml -> write_mzml` of the same data
//! produces a structurally identical document (modulo whitespace
//! differences inside element bodies and the indexed offset list, which
//! is reconstructed during write).

mod reader;
mod writer;

pub use reader::{parse_bytes, read_mzml, MzmlData, Verbatim};
pub use writer::write_mzml;
