//! Vendor ingest: a single entry point that handles every supported
//! vendor format via [`openmassspec_io`].
//!
//! Replaces the per-vendor `thermo.rs`, `bruker.rs`, `waters.rs` adapters.
//! Each of those used to talk directly to its vendor crate and decode peak
//! arrays itself; this module delegates to `openmassspec_io::collect` which
//! drives the canonical `SpectrumSource` implementation for each vendor.
//!
//! No mzML intermediate is produced - vendor records (`SpectrumRecord`) are
//! converted directly into SpecLance [`Spectrum`] / [`Run`] rows. This saves
//! both the CPU cost of XML / base64 encoding and the memory cost of
//! buffering the document.

use std::path::Path;

use openmassspec_io::core::{Polarity, RunMetadata, ScanMode, SpectrumRecord};
use openmassspec_io::{detect_format, VendorFormat};
use sha2::{Digest, Sha256};
use speclance_core::{Chromatogram, Precursor, Run, Spectrum};

use crate::error::{MsError, MsResult};
use crate::mzml::MzmlData;

/// Detect the vendor format at `path` and ingest the bundle into a
/// [`MzmlData`]. The container is named after mzML for historical reasons but
/// is now the canonical in-memory shape every SpecLance ingester (mzML reader
/// included) produces.
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
    };

    let source_path = detected.path.to_string_lossy().to_string();
    let size = path_size(&detected.path);
    let run_id = derive_run_id(&source_path, size);

    let (records, meta) = openmassspec_io::collect(detected)
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
        instrument: Some(meta.instrument.name.clone()),
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
        .map(|a| activation_name(a).to_string());

    // `ce_is_nce` flips the unit; SpecLance's column is just an f32, so we drop
    // normalized values rather than report them in the wrong unit.
    let collision_energy = rec
        .precursor
        .as_ref()
        .and_then(|p| {
            if p.ce_is_nce {
                None
            } else {
                p.collision_energy
            }
        })
        .map(|e| e as f32);

    Spectrum {
        run_id: run_id.to_string(),
        scan_num: rec.scan_number,
        native_id: Some(rec.native_id.clone()),
        ms_level: rec.ms_level as u8,
        rt: Some(rec.retention_time_sec),
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
        scan_window_lower: rec.low_mz,
        scan_window_upper: rec.high_mz,
        mz: rec.mz.clone(),
        intensity: rec.intensity.clone(),
        cv_params: None,
    }
}

fn activation_name(act: openmassspec_io::core::Activation) -> &'static str {
    use openmassspec_io::core::Activation as A;
    match act {
        A::CID => "collision-induced dissociation",
        A::HCD => "beam-type collision-induced dissociation",
        A::ETD | A::EThcD => "electron transfer dissociation",
        A::MPID => "supplemental beam-type collision-induced dissociation",
        A::ECD => "electron capture dissociation",
        A::IRMPD => "infrared multiphoton dissociation",
        A::PD => "in-source collision-induced dissociation",
        A::PQD => "pulsed q dissociation",
        A::UVPD => "ultraviolet photodissociation",
        A::SID => "beam-type collision-induced dissociation",
    }
}

/// Total bytes on disk for a vendor bundle. Handles both single-file
/// (Thermo `.raw`) and directory (Bruker `.d/`, Waters `.raw/`) layouts.
fn path_size(path: &Path) -> u64 {
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.is_file() {
            return meta.len();
        }
    }
    let mut total = 0u64;
    if let Ok(rd) = std::fs::read_dir(path) {
        for entry in rd.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total += meta.len();
                }
            }
        }
    }
    total
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
