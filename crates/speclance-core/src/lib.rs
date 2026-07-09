//! SpecLance core: Arrow schema, in-memory record types, and the Lance store.
//!
//! See [`schema`] for the on-disk Arrow schemas, [`types`] for the in-memory
//! representations used during ingest, and [`store`] for the [`Store`] handle.

pub mod error;
pub mod from_batch;
pub mod schema;
pub mod store;
pub mod types;

pub use error::{Error, Result};
pub use from_batch::{batches_to_chromatograms, batches_to_runs, batches_to_spectra};
pub use store::Store;
pub use types::{Chromatogram, Precursor, Run, Spectrum};
