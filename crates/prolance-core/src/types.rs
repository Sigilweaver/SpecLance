//! In-memory record types used during ingest, before being converted to
//! Arrow RecordBatches and written to a Lance table.

use serde::{Deserialize, Serialize};

/// A single mass spectrum.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Spectrum {
    pub run_id: String,
    pub scan_num: u32,
    pub native_id: Option<String>,
    pub ms_level: u8,
    pub rt: Option<f64>,
    pub tic: Option<f64>,
    pub base_peak_mz: Option<f64>,
    pub base_peak_intensity: Option<f64>,
    pub polarity: Option<i8>,
    pub centroided: Option<bool>,
    pub precursor: Option<Precursor>,
    pub activation: Option<String>,
    pub collision_energy: Option<f32>,
    pub inv_mobility: Option<f64>,
    pub mz_precision: Option<u8>,
    pub intensity_precision: Option<u8>,
    pub scan_window_lower: Option<f64>,
    pub scan_window_upper: Option<f64>,
    pub mz: Vec<f64>,
    pub intensity: Vec<f32>,
    /// JSON blob of CV params not promoted to typed columns.
    pub cv_params: Option<String>,
}

/// Precursor information for MS2+ spectra.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Precursor {
    pub mz: Option<f64>,
    pub charge: Option<i8>,
    pub intensity: Option<f64>,
    pub isolation_window_target: Option<f64>,
    pub isolation_window_lower: Option<f64>,
    pub isolation_window_upper: Option<f64>,
}

/// Run-level metadata (one per ingested file).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Run {
    pub run_id: String,
    pub source_path: Option<String>,
    pub source_format: String,
    pub instrument: Option<String>,
    pub start_time: Option<String>,
    pub ingested_at: Option<String>,
    pub spectrum_count: Option<u32>,
    pub ms1_count: Option<u32>,
    pub ms2_count: Option<u32>,
    /// JSON blob of full mzML run-level metadata for verbatim roundtrip.
    pub run_metadata: Option<String>,
}

/// A chromatogram trace (TIC, BPC, SRM/MRM transition).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Chromatogram {
    pub run_id: String,
    pub chrom_id: String,
    pub chrom_type: Option<String>,
    pub precursor_mz: Option<f64>,
    pub product_mz: Option<f64>,
    pub time: Vec<f32>,
    pub intensity: Vec<f32>,
    pub cv_params: Option<String>,
}

/// Convenience: compute a stable run_id from a source path and mtime.
pub fn run_id_for_path(path: &str, mtime_unix: i64, size: u64) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(path.as_bytes());
    h.update(mtime_unix.to_le_bytes());
    h.update(size.to_le_bytes());
    let digest = h.finalize();
    let mut out = String::with_capacity(16);
    for b in &digest[..8] {
        out.push_str(&format!("{:02x}", b));
    }
    out
}
