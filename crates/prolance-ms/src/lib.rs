//! ProLance ingest adapters and mzML reader/writer.
//!
//! The mzML codec is built-in; vendor adapters are gated behind cargo
//! features so the crate compiles cleanly even without the underlying
//! reader crates available.

pub mod error;
pub mod mzml;

pub use error::{MsError, MsResult};

#[cfg(feature = "thermo")]
pub mod thermo;

#[cfg(feature = "bruker")]
pub mod bruker;

#[cfg(feature = "waters")]
pub mod waters;

pub use mzml::{read_mzml, write_mzml};
