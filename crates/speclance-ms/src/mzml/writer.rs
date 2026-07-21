//! mzML writer.
//!
//! Thin adapter over `openmassspec_core::write_mzml`. Converts SpecLance's
//! column-store records (`Run`, `&[Spectrum]`) into
//! `openmassspec_core::{RunMetadata, SpectrumRecord}` and delegates to the
//! canonical writer. Output is semantically equivalent (not byte
//! identical) to whatever the source produced - this is the agreed
//! roundtrip contract for SpecLance.
//!
//! Chromatograms are currently not emitted: `openmassspec_core::write_mzml`
//! is spectrum-centric. The caller-facing signature still accepts a
//! chromatogram slice for source compatibility; a non-empty slice is
//! ignored with no error. Re-introduce chromatogram emission via a
//! dedicated upstream change to `openmassspec-core` if it becomes
//! necessary.

use std::io::Write;

use openmassspec_io::core::{
    CvTerm, MobilityArrayKind, Polarity, PrecursorInfo, RunMetadata, ScanMode, SpectrumRecord,
};
use openmassspec_io::VecSource;
use speclance_core::{Chromatogram, Run, Spectrum};

use crate::error::{MsError, MsResult};

/// Serialize a run + its spectra to mzML using the canonical
/// `openmassspec_core` writer. Chromatograms are accepted for API
/// compatibility but not currently emitted.
pub fn write_mzml<W: Write>(
    out: &mut W,
    run: &Run,
    spectra: &[Spectrum],
    _chromatograms: &[Chromatogram],
) -> MsResult<()> {
    let metadata = run_to_metadata(run);
    let records: Vec<SpectrumRecord> = spectra
        .iter()
        .enumerate()
        .map(|(idx, s)| spectrum_to_record(idx, s))
        .collect();

    let mut src = VecSource::new(metadata, records);
    openmassspec_io::core::write_mzml(&mut src, out)
        .map_err(|e| MsError::Other(format!("openmassspec-core write_mzml: {e}")))?;
    Ok(())
}

fn run_to_metadata(run: &Run) -> RunMetadata {
    let source_file_name = run
        .source_path
        .as_deref()
        .and_then(|p| {
            std::path::Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| run.run_id.clone());

    let (source_file_format, native_id_format) = format_terms(&run.source_format);

    RunMetadata {
        source_file_name,
        source_file_format,
        native_id_format,
        instrument: instrument_term(run.instrument.as_deref()),
        software_name: "SpecLance".to_string(),
        software_version: env!("CARGO_PKG_VERSION").to_string(),
        start_timestamp: run.start_time.clone(),
        mobility_array_kind: Some(MobilityArrayKind::InverseReducedVsPerCm2),
    }
}

fn format_terms(source_format: &str) -> (CvTerm, CvTerm) {
    match source_format {
        "thermo-raw" => (
            CvTerm::new("MS:1000563", "Thermo RAW format"),
            CvTerm::new("MS:1000768", "Thermo nativeID format"),
        ),
        "bruker-tdf" => (
            CvTerm::new("MS:1002817", "Bruker TDF format"),
            CvTerm::new("MS:1002818", "Bruker TDF nativeID format"),
        ),
        "waters-raw" => (
            CvTerm::new("MS:1000526", "Waters raw format"),
            CvTerm::new("MS:1000769", "Waters nativeID format"),
        ),
        _ => (
            CvTerm::new("MS:1000584", "mzML format"),
            CvTerm::new("MS:1000774", "multiple peak list nativeID format"),
        ),
    }
}

fn instrument_term(name: Option<&str>) -> CvTerm {
    CvTerm::new("MS:1000031", name.unwrap_or("instrument model").to_string())
}

fn spectrum_to_record(idx: usize, s: &Spectrum) -> SpectrumRecord {
    let polarity = match s.polarity {
        Some(1) => Some(Polarity::Positive),
        Some(-1) => Some(Polarity::Negative),
        _ => None,
    };
    let scan_mode = s.centroided.map(|c| {
        if c {
            ScanMode::Centroid
        } else {
            ScanMode::Profile
        }
    });

    let native_id = s
        .native_id
        .clone()
        .unwrap_or_else(|| format!("scan={}", s.scan_num));

    let precursor = s.precursor.as_ref().map(|p| precursor_to_info(p, s));

    SpectrumRecord {
        index: idx,
        scan_number: s.scan_num,
        native_id,
        ms_level: s.ms_level as u32,
        polarity,
        scan_mode,
        analyzer: None,
        filter: None,
        retention_time_sec: s.rt.unwrap_or(0.0),
        total_ion_current: s.tic,
        base_peak_mz: s.base_peak_mz,
        base_peak_intensity: s.base_peak_intensity,
        low_mz: s.scan_window_lower,
        high_mz: s.scan_window_upper,
        ion_injection_time_ms: None,
        inv_mobility: s.inv_mobility,
        precursor,
        mz: s.mz.clone(),
        intensity: s.intensity.clone(),
        inv_mobility_per_peak: None,
        // SpecLance has no FAIMS ingest path on any vendor adapter today;
        // `openmassspec-core` 1.2.0 added this field with no `Default` impl
        // on `SpectrumRecord`, so every construction site needs it spelled
        // out explicitly.
        faims_cv: None,
    }
}

fn precursor_to_info(p: &speclance_core::Precursor, s: &Spectrum) -> PrecursorInfo {
    // SpecLance stores symmetric lower/upper half-widths around the target.
    // Restore the full width when both are present.
    let isolation_width = match (p.isolation_window_lower, p.isolation_window_upper) {
        (Some(lo), Some(hi)) => Some(lo + hi),
        _ => None,
    };
    PrecursorInfo {
        target_mz: p.isolation_window_target,
        selected_mz: p.mz,
        isolation_width,
        charge: p.charge.map(|z| z as i32),
        intensity: p.intensity,
        collision_energy: s.collision_energy.map(|e| e as f64),
        ce_is_nce: false,
        precursor_native_id: None,
        activation: s.activation.as_deref().and_then(parse_activation),
        analyzer: None,
    }
}

fn parse_activation(name: &str) -> Option<openmassspec_io::core::Activation> {
    use openmassspec_io::core::Activation as A;
    let n = name.to_ascii_lowercase();
    if n.contains("electron transfer") {
        Some(A::ETD)
    } else if n.contains("electron capture") {
        Some(A::ECD)
    } else if n.contains("beam-type") {
        Some(A::HCD)
    } else if n.contains("collision-induced") {
        Some(A::CID)
    } else if n.contains("infrared multiphoton") {
        Some(A::IRMPD)
    } else if n.contains("ultraviolet") {
        Some(A::UVPD)
    } else if n.contains("pulsed q") {
        Some(A::PQD)
    } else if n.contains("in-source") {
        Some(A::PD)
    } else {
        None
    }
}
