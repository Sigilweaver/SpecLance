//! Vendor ingest: a single entry point that handles every supported
//! vendor format via [`openmassspec_io`].
//!
//! This module does not decode vendor bytes itself; it delegates to
//! `openmassspec_io::collect`, which drives the canonical `SpectrumSource`
//! implementation for each vendor.
//!
//! No mzML intermediate is produced - vendor records (`SpectrumRecord`) are
//! converted directly into SpecLance [`Spectrum`] / [`Run`] rows. This saves
//! both the CPU cost of XML / base64 encoding and the memory cost of
//! buffering the document.

use std::path::Path;

use openmassspec_io::core::{CollisionEnergyUnit, Polarity, RunMetadata, ScanMode, SpectrumRecord};
use openmassspec_io::{detect_format, VendorFormat};
use sha2::{Digest, Sha256};
use speclance_core::{Chromatogram, Precursor, Run, Spectrum};

use crate::error::{MsError, MsResult};
use crate::mzml::MzmlData;

/// Detect the vendor format at `path` and ingest the bundle into a
/// [`MzmlData`]. `MzmlData` is the in-memory shape every SpecLance ingester
/// (the mzML reader included) produces.
pub fn ingest<P: AsRef<Path>>(path: P) -> MsResult<MzmlData> {
    let path = path.as_ref();
    let detected = detect_format(path).ok_or_else(|| {
        MsError::Unsupported(format!(
            "no supported vendor format detected at {}",
            path.display()
        ))
    })?;

    let source_format = match detected.format {
        VendorFormat::ThermoRaw => "thermo-raw",
        VendorFormat::BrukerTdf => "bruker-tdf",
        VendorFormat::WatersRaw => "waters-raw",
        VendorFormat::AgilentMassHunter => "agilent-masshunter",
        VendorFormat::SciexWiff => "sciex-wiff",
        VendorFormat::ShimadzuLabSolutions => "shimadzu-labsolutions",
        // `VendorFormat` is #[non_exhaustive]: refuse formats SpecLance has
        // no source_format label for instead of guessing one.
        other => {
            return Err(MsError::Unsupported(format!(
                "vendor format {other:?} has no SpecLance source_format label"
            )))
        }
    };

    let source_path = detected.path.to_string_lossy().to_string();
    let size = path_size(&detected.path)?;
    let run_id = derive_run_id(&source_path, size);

    let (records, meta) = openmassspec_io::collect(&detected)
        .map_err(|e| MsError::Other(format!("openmassspec-io collect: {e}")))?;

    let mut ms1_count = 0u32;
    let mut ms2_count = 0u32;
    let mut spectra: Vec<Spectrum> = Vec::with_capacity(records.len());
    for rec in &records {
        match rec.ms_level {
            1 => ms1_count += 1,
            l if l >= 2 => ms2_count += 1,
            _ => {}
        }
        spectra.push(record_to_spectrum(&run_id, rec));
    }

    let run = build_run(
        run_id,
        source_path,
        source_format,
        &meta,
        spectra.len() as u32,
        ms1_count,
        ms2_count,
    );

    Ok(MzmlData {
        run,
        spectra,
        chromatograms: Vec::<Chromatogram>::new(),
    })
}

fn build_run(
    run_id: String,
    source_path: String,
    source_format: &str,
    meta: &RunMetadata,
    spectrum_count: u32,
    ms1_count: u32,
    ms2_count: u32,
) -> Run {
    Run {
        run_id,
        source_path: Some(source_path),
        source_format: source_format.into(),
        instrument: Some(meta.instrument.name.to_string()),
        start_time: meta.start_timestamp.clone(),
        ingested_at: Some(chrono::Utc::now().to_rfc3339()),
        spectrum_count: Some(spectrum_count),
        ms1_count: Some(ms1_count),
        ms2_count: Some(ms2_count),
        run_metadata: None,
    }
}

fn record_to_spectrum(run_id: &str, rec: &SpectrumRecord) -> Spectrum {
    let polarity = match rec.polarity {
        Some(Polarity::Positive) => Some(1i8),
        Some(Polarity::Negative) => Some(-1i8),
        _ => None,
    };
    let centroided = match rec.scan_mode {
        Some(ScanMode::Centroid) => Some(true),
        Some(ScanMode::Profile) => Some(false),
        _ => None,
    };

    let precursor = rec.precursor.as_ref().map(|pre| {
        let half = pre.isolation_width.map(|w| w / 2.0);
        Precursor {
            mz: pre.selected_mz,
            charge: pre.charge.map(|z| z as i8),
            intensity: pre.intensity,
            isolation_window_target: pre.target_mz,
            isolation_window_lower: half,
            isolation_window_upper: half,
        }
    });

    let activation = rec
        .precursor
        .as_ref()
        .and_then(|p| p.activation)
        .map(crate::activation::activation_label);

    let collision_energy = column_collision_energy(rec);
    let scan_window = column_scan_window(rec);
    let cv_params = kept_in_cv_params(rec, collision_energy.is_some(), scan_window.is_some());

    Spectrum {
        run_id: run_id.to_string(),
        scan_num: rec.scan_number,
        native_id: Some(rec.native_id.clone()),
        ms_level: rec.ms_level as u8,
        // A missing retention time stays missing; it is never written as 0.
        rt: rec.retention_time_sec,
        tic: rec.total_ion_current,
        base_peak_mz: rec.base_peak_mz,
        base_peak_intensity: rec.base_peak_intensity,
        polarity,
        centroided,
        precursor,
        activation,
        collision_energy,
        inv_mobility: rec.inv_mobility,
        mz_precision: Some(64),
        intensity_precision: Some(32),
        scan_window_lower: scan_window.map(|w| w.0),
        scan_window_upper: scan_window.map(|w| w.1),
        mz: rec.mz.clone(),
        intensity: rec.intensity.clone(),
        cv_params,
    }
}

/// The value of the `collision_energy` column: one energy whose unit is
/// electronvolts. The column is a bare number in eV, so a normalized energy,
/// an energy whose unit the file does not state, and stepped energies are
/// not stored here; [`kept_in_cv_params`] keeps them.
fn column_collision_energy(rec: &SpectrumRecord) -> Option<f32> {
    match rec.precursor.as_ref()?.collision_energies.as_slice() {
        [ce] if ce.unit == Some(CollisionEnergyUnit::Ev) => Some(ce.value as f32),
        _ => None,
    }
}

/// The instrument's acquisition range, when the reader states exactly one.
/// The observed m/z range (`low_mz`, `high_mz`) is a different quantity and
/// is not stored as the scan window.
fn column_scan_window(rec: &SpectrumRecord) -> Option<(f64, f64)> {
    match rec.scan_windows.as_slice() {
        [w] => Some((w.lower, w.upper)),
        _ => None,
    }
}

/// What the typed columns cannot hold, as the `cv_params` JSON blob: the
/// collision energies (with units) when `collision_energy` is empty, the scan
/// windows when `scan_window_*` is empty, and the codes of the warnings the
/// reader put on the record. Nothing the reader stated is dropped on the way
/// into the store.
fn kept_in_cv_params(
    rec: &SpectrumRecord,
    has_ce_column: bool,
    has_window_columns: bool,
) -> Option<String> {
    let mut kept = serde_json::Map::new();
    if !has_ce_column {
        if let Some(p) = rec
            .precursor
            .as_ref()
            .filter(|p| !p.collision_energies.is_empty())
        {
            let list: Vec<_> = p
                .collision_energies
                .iter()
                .map(|ce| {
                    serde_json::json!({
                        "value": ce.value,
                        "unit": ce.unit.map(CollisionEnergyUnit::as_str),
                    })
                })
                .collect();
            kept.insert("openmassspec.collision_energies".into(), list.into());
        }
    }
    if !has_window_columns && !rec.scan_windows.is_empty() {
        let list: Vec<_> = rec
            .scan_windows
            .iter()
            .map(|w| serde_json::json!({"lower": w.lower, "upper": w.upper}))
            .collect();
        kept.insert("openmassspec.scan_windows".into(), list.into());
    }
    if !rec.warnings.is_empty() {
        let codes: Vec<_> = rec.warnings.iter().map(|w| w.kind.code()).collect();
        kept.insert("openmassspec.warnings".into(), codes.into());
    }
    (!kept.is_empty()).then(|| serde_json::Value::Object(kept).to_string())
}

/// Total bytes on disk for a vendor bundle. Handles both single-file
/// (Thermo `.raw`) and directory (Bruker `.d/`, Waters `.raw/`) layouts.
/// The size feeds the run ID, so an unreadable path or entry is an error
/// rather than a silently smaller total.
fn path_size(path: &Path) -> MsResult<u64> {
    let meta = std::fs::metadata(path)?;
    if meta.is_file() {
        return Ok(meta.len());
    }
    let mut total = 0u64;
    for entry in std::fs::read_dir(path)? {
        let meta = entry?.metadata()?;
        if meta.is_file() {
            total += meta.len();
        }
    }
    Ok(total)
}

/// Stable run identifier derived from the source path and total bundle size.
/// Mirrors the mzML reader's `derive_run_id` so the two ingest paths produce
/// identical `run_id`s for the same source file.
fn derive_run_id(source_path: &str, size: u64) -> String {
    let mut h = Sha256::new();
    h.update(source_path.as_bytes());
    h.update(size.to_le_bytes());
    let d = h.finalize();
    let mut out = String::with_capacity(16);
    for b in &d[..8] {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_size_sums_files_in_a_bundle_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.bin"), [0u8; 10]).unwrap();
        std::fs::write(dir.path().join("b.bin"), [0u8; 5]).unwrap();
        assert_eq!(path_size(dir.path()).unwrap(), 15);
        assert_eq!(path_size(&dir.path().join("a.bin")).unwrap(), 10);
    }

    use openmassspec_io::core::{PrecursorInfo, Warning, WarningKind};

    fn record() -> SpectrumRecord {
        SpectrumRecord::new(0, 1, "scan=1")
            .with_ms_level(2)
            .with_scan_mode(ScanMode::Centroid)
            .with_retention_time_sec(12.5)
            .with_peaks(vec![100.0, 200.0], vec![1.0, 2.0])
    }

    fn cv(s: &Spectrum) -> serde_json::Value {
        serde_json::from_str(s.cv_params.as_deref().expect("cv_params kept")).unwrap()
    }

    #[test]
    fn a_missing_retention_time_stays_missing() {
        let with = record_to_spectrum("r", &record());
        assert_eq!(with.rt, Some(12.5));
        let mut rec = record();
        rec.retention_time_sec = None;
        assert_eq!(record_to_spectrum("r", &rec).rt, None);
    }

    #[test]
    fn one_electronvolt_energy_fills_the_column() {
        let rec = record().with_precursor(
            PrecursorInfo::default().with_collision_energy(35.0, CollisionEnergyUnit::Ev),
        );
        let s = record_to_spectrum("r", &rec);
        assert_eq!(s.collision_energy, Some(35.0));
        assert!(s.cv_params.is_none(), "{:?}", s.cv_params);
    }

    #[test]
    fn other_energies_stay_out_of_the_ev_column_and_are_kept() {
        let cases = [
            (
                PrecursorInfo::default().with_collision_energy(30.0, CollisionEnergyUnit::Nce),
                serde_json::json!([{"value": 30.0, "unit": "nce"}]),
            ),
            (
                // A value whose unit the file does not state is not eV.
                PrecursorInfo::default().with_collision_energy(30.0, None::<CollisionEnergyUnit>),
                serde_json::json!([{"value": 30.0, "unit": null}]),
            ),
            (
                PrecursorInfo::default()
                    .with_collision_energy(20.0, CollisionEnergyUnit::Ev)
                    .with_collision_energy(30.0, CollisionEnergyUnit::Ev),
                serde_json::json!([
                    {"value": 20.0, "unit": "ev"},
                    {"value": 30.0, "unit": "ev"}
                ]),
            ),
        ];
        for (precursor, kept) in cases {
            let s = record_to_spectrum("r", &record().with_precursor(precursor));
            assert_eq!(s.collision_energy, None);
            assert_eq!(cv(&s)["openmassspec.collision_energies"], kept);
        }
    }

    #[test]
    fn the_scan_window_is_the_stated_range_not_the_observed_one() {
        let mut rec = record().with_scan_window(50.0, 1500.0);
        rec.low_mz = Some(100.0);
        rec.high_mz = Some(200.0);
        let s = record_to_spectrum("r", &rec);
        assert_eq!(
            (s.scan_window_lower, s.scan_window_upper),
            (Some(50.0), Some(1500.0))
        );

        // No stated window: the observed range does not stand in for it.
        let mut rec = record();
        rec.low_mz = Some(100.0);
        rec.high_mz = Some(200.0);
        let s = record_to_spectrum("r", &rec);
        assert_eq!((s.scan_window_lower, s.scan_window_upper), (None, None));
        assert!(s.cv_params.is_none());

        // Several windows do not fit two columns and are kept.
        let rec = record()
            .with_scan_window(50.0, 500.0)
            .with_scan_window(500.0, 1500.0);
        let s = record_to_spectrum("r", &rec);
        assert_eq!(s.scan_window_lower, None);
        assert_eq!(
            cv(&s)["openmassspec.scan_windows"],
            serde_json::json!([
                {"lower": 50.0, "upper": 500.0},
                {"lower": 500.0, "upper": 1500.0}
            ])
        );
    }

    #[test]
    fn reader_warnings_are_kept_by_code() {
        let rec = record().with_warning(Warning::new(WarningKind::EmptySpectrum, "no peaks"));
        let s = record_to_spectrum("r", &rec);
        assert_eq!(
            cv(&s)["openmassspec.warnings"],
            serde_json::json!(["empty_spectrum"])
        );
    }

    #[test]
    fn path_size_reports_a_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let err = path_size(&dir.path().join("absent.raw")).unwrap_err();
        assert!(matches!(err, MsError::Io(_)), "{err}");
    }
}
