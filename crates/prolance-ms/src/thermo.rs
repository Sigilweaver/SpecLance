//! Thermo Fisher `.raw` ingest adapter.
//!
//! Populates [`MzmlData`] (the canonical in-memory bundle the rest of
//! ProLance consumes, regardless of source) directly from
//! [`opentfraw::SpectrumRecord`]s. No XML round trip: the .raw file is read
//! once, decoded, and turned into [`Spectrum`] records without ever
//! materialising an mzML buffer. For multi-GB Thermo files this saves both
//! the CPU cost of base64 encode/decode and the RAM cost of buffering the
//! XML document.

use std::fs::File;
use std::path::Path;

use opentfraw::{iter_spectra, RawFileReader};
use prolance_core::{Chromatogram, Precursor, Run, Spectrum};
use sha2::{Digest, Sha256};

use crate::error::{MsError, MsResult};
use crate::mzml::MzmlData;

/// Ingest a Thermo `.raw` file into ProLance records.
pub fn ingest<P: AsRef<Path>>(path: P) -> MsResult<MzmlData> {
    let path = path.as_ref();
    let raw =
        RawFileReader::open_path(path).map_err(|e| MsError::Other(format!("opentfraw open: {e}")))?;
    let mut source = File::open(path).map_err(MsError::Io)?;

    let source_path = path.to_string_lossy().to_string();
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let run_id = derive_run_id(&source_path, size);

    let mut spectra: Vec<Spectrum> = Vec::with_capacity(raw.num_scans as usize);
    let mut ms1_count: u32 = 0;
    let mut ms2_count: u32 = 0;

    for rec in iter_spectra(&raw, &mut source, false) {
        match rec.ms_level {
            1 => ms1_count += 1,
            l if l >= 2 => ms2_count += 1,
            _ => {}
        }
        spectra.push(record_to_spectrum(&run_id, &rec));
    }

    let instrument = raw.instrument_model.map(|s| s.to_string());
    // sample_info.start_time is a Thermo MS-Excel-style serial date (days since
    // 1899-12-30). Surface it as-is in a stringified form; downstream metadata
    // pipelines can convert if they need a real timestamp.
    let start_time_serial = raw.run_header.sample_info.start_time;
    let start_time = if start_time_serial > 0.0 {
        Some(format!("{:.6}", start_time_serial))
    } else {
        None
    };

    let run = Run {
        run_id,
        source_path: Some(source_path),
        source_format: "thermo-raw".into(),
        instrument,
        start_time,
        ingested_at: Some(chrono::Utc::now().to_rfc3339()),
        spectrum_count: Some(spectra.len() as u32),
        ms1_count: Some(ms1_count),
        ms2_count: Some(ms2_count),
        run_metadata: None,
    };

    Ok(MzmlData {
        run,
        spectra,
        chromatograms: Vec::<Chromatogram>::new(),
    })
}

fn record_to_spectrum(run_id: &str, rec: &opentfraw::SpectrumRecord) -> Spectrum {
    let polarity = match rec.polarity {
        Some(opentfraw::Polarity::Positive) => Some(1i8),
        Some(opentfraw::Polarity::Negative) => Some(-1i8),
        _ => None,
    };
    let centroided = match rec.scan_mode {
        Some(opentfraw::ScanMode::Centroid) => Some(true),
        Some(opentfraw::ScanMode::Profile) => Some(false),
        _ => None,
    };

    let precursor = rec.precursor.as_ref().map(|pre| Precursor {
        mz: pre.selected_mz,
        charge: pre.charge.map(|z| z as i8),
        intensity: None,
        isolation_window_target: pre.target_mz,
        isolation_window_lower: pre.isolation_width.map(|w| w / 2.0),
        isolation_window_upper: pre.isolation_width.map(|w| w / 2.0),
    });

    let activation = rec
        .precursor
        .as_ref()
        .and_then(|p| p.activation)
        .map(|a| activation_name(a).to_string());

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

    let native_id = Some(format!(
        "controllerType=0 controllerNumber=1 scan={}",
        rec.scan_number
    ));

    Spectrum {
        run_id: run_id.to_string(),
        scan_num: rec.scan_number,
        native_id,
        ms_level: rec.ms_level as u8,
        rt: Some(rec.retention_time_min * 60.0),
        tic: Some(rec.total_ion_current),
        base_peak_mz: Some(rec.base_peak_mz),
        base_peak_intensity: Some(rec.base_peak_intensity),
        polarity,
        centroided,
        precursor,
        activation,
        collision_energy,
        inv_mobility: None,
        mz_precision: Some(64),
        intensity_precision: Some(32),
        scan_window_lower: Some(rec.low_mz),
        scan_window_upper: Some(rec.high_mz),
        mz: rec.mz.clone(),
        intensity: rec.intensity.clone(),
        cv_params: None,
    }
}

fn activation_name(act: opentfraw::Activation) -> &'static str {
    use opentfraw::Activation as A;
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

/// Stable run identifier derived from the source path and size.
///
/// Mirrors the mzML reader's `derive_run_id` so the two ingest paths produce
/// identical run_ids for the same source file.
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
