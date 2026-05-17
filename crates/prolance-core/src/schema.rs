//! Arrow schemas for the ProLance store.
//!
//! ProLance has three tables:
//! - `runs`         — one row per ingested file
//! - `spectra`      — one row per spectrum (peak arrays as list columns)
//! - `chromatograms`— one row per chromatogram trace (TIC / BPC / SRM)

use arrow_schema::{DataType, Field, Schema};
use std::sync::Arc;

/// Table name for the runs registry.
pub const RUNS_TABLE: &str = "runs";

/// Table name for spectra.
pub const SPECTRA_TABLE: &str = "spectra";

/// Table name for chromatograms.
pub const CHROMATOGRAMS_TABLE: &str = "chromatograms";

/// One row per ingested file. `run_metadata` holds full mzML-level
/// metadata (instrument config, data processing list, sample list,
/// software list, etc.) as a JSON blob so the roundtrip can re-emit
/// them verbatim.
pub fn runs_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("run_id", DataType::Utf8, false),
        Field::new("source_path", DataType::Utf8, true),
        Field::new("source_format", DataType::Utf8, false),
        Field::new("instrument", DataType::Utf8, true),
        Field::new("start_time", DataType::Utf8, true), // ISO8601
        Field::new("ingested_at", DataType::Utf8, true),
        Field::new("spectrum_count", DataType::UInt32, true),
        Field::new("ms1_count", DataType::UInt32, true),
        Field::new("ms2_count", DataType::UInt32, true),
        Field::new("run_metadata", DataType::Utf8, true),
    ]))
}

/// One row per spectrum. Peak arrays are stored as Arrow `LargeList`
/// columns so each spectrum is self-contained and atomic to retrieve.
pub fn spectra_schema() -> Arc<Schema> {
    let mz_item = Arc::new(Field::new("item", DataType::Float64, false));
    let int_item = Arc::new(Field::new("item", DataType::Float32, false));
    Arc::new(Schema::new(vec![
        Field::new("run_id", DataType::Utf8, false),
        Field::new("scan_num", DataType::UInt32, false),
        Field::new("native_id", DataType::Utf8, true),
        Field::new("ms_level", DataType::UInt8, false),
        Field::new("rt", DataType::Float64, true),
        Field::new("tic", DataType::Float64, true),
        Field::new("base_peak_mz", DataType::Float64, true),
        Field::new("base_peak_intensity", DataType::Float64, true),
        Field::new("polarity", DataType::Int8, true), // +1/-1
        Field::new("centroided", DataType::Boolean, true),
        Field::new("precursor_mz", DataType::Float64, true),
        Field::new("precursor_charge", DataType::Int8, true),
        Field::new("precursor_intensity", DataType::Float64, true),
        Field::new("isolation_window_target", DataType::Float64, true),
        Field::new("isolation_window_lower", DataType::Float64, true),
        Field::new("isolation_window_upper", DataType::Float64, true),
        Field::new("activation", DataType::Utf8, true),
        Field::new("collision_energy", DataType::Float32, true),
        Field::new("inv_mobility", DataType::Float64, true),
        Field::new("mz_precision", DataType::UInt8, true), // 32 or 64
        Field::new("intensity_precision", DataType::UInt8, true),
        Field::new("scan_window_lower", DataType::Float64, true),
        Field::new("scan_window_upper", DataType::Float64, true),
        Field::new("mz", DataType::LargeList(mz_item), false),
        Field::new("intensity", DataType::LargeList(int_item), false),
        Field::new("cv_params", DataType::Utf8, true), // JSON
    ]))
}

/// One row per chromatogram trace.
pub fn chromatograms_schema() -> Arc<Schema> {
    let time_item = Arc::new(Field::new("item", DataType::Float32, false));
    let int_item = Arc::new(Field::new("item", DataType::Float32, false));
    Arc::new(Schema::new(vec![
        Field::new("run_id", DataType::Utf8, false),
        Field::new("chrom_id", DataType::Utf8, false),
        Field::new("chrom_type", DataType::Utf8, true),
        Field::new("precursor_mz", DataType::Float64, true),
        Field::new("product_mz", DataType::Float64, true),
        Field::new("time", DataType::LargeList(time_item), false),
        Field::new("intensity", DataType::LargeList(int_item), false),
        Field::new("cv_params", DataType::Utf8, true),
    ]))
}
