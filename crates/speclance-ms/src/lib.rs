//! SpecLance ingest adapters and mzML reader/writer.
//!
//! The mzML codec is built-in. Vendor ingest is a single module
//! ([`vendor`]) that drives [`openmassspec_io`] - no direct vendor-crate
//! dependencies live here.

pub mod error;
pub mod mzml;

pub use error::{MsError, MsResult};

#[cfg(feature = "vendors")]
pub mod vendor;

pub use mzml::{read_mzml, write_mzml};
