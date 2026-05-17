//! Bruker timsTOF `.d/` ingest adapter, built directly on [`opentimstdf`].
//!
//! No mzML intermediate. The adapter opens the bundle, decodes each frame's
//! TOF/intensity peaks once, applies the linear m/z calibration, then emits
//! one [`Spectrum`] per logical scan:
//!
//! * **MS1 frames** (`msms_type == 0`): all mobility scans within the frame
//!   are pooled into a single MS1 spectrum, sorted by m/z. Per-peak mobility
//!   information is collapsed (the column-store schema only has a scalar
//!   `inv_mobility` per spectrum); this matches what mzML writers like
//!   `tdf2mzml` do by default.
//! * **PASEF DDA frames** (`msms_type == 8`): one MS2 spectrum per
//!   `PasefMsMsInfo` row, filtered to that row's mobility scan range, with
//!   precursor m/z / charge / intensity copied from the `Precursors` table.
//! * **diaPASEF frames** (`msms_type == 9`): one MS2 spectrum per
//!   `DiaWindow` in the frame, with the isolation window taken from the
//!   `DiaFrameMsMsWindows` table.
//!
//! `scan_num` is a monotonic 1-based counter across the whole bundle (PASEF
//! frames produce many spectra per frame, so frame ID alone is not unique).

use std::path::Path;

use opentimstdf::{
    Calibration, DiaWindow, Frame, PasefMsMsInfo, Peak, Precursor as TdfPrecursor, Reader,
};
use prolance_core::{Chromatogram, Precursor, Run, Spectrum};
use sha2::{Digest, Sha256};

use crate::error::{MsError, MsResult};
use crate::mzml::MzmlData;

/// Ingest a Bruker timsTOF `.d/` bundle into ProLance records.
pub fn ingest<P: AsRef<Path>>(path: P) -> MsResult<MzmlData> {
    let path = path.as_ref();
    let reader =
        Reader::open(path).map_err(|e| MsError::Other(format!("opentimstdf open: {e}")))?;

    let metadata = reader
        .metadata()
        .map_err(|e| MsError::Other(format!("opentimstdf metadata: {e}")))?;
    let calibration = reader
        .calibration()
        .map_err(|e| MsError::Other(format!("opentimstdf calibration: {e}")))?;
    let frames = reader
        .frames()
        .map_err(|e| MsError::Other(format!("opentimstdf frames: {e}")))?;

    let source_path = path.to_string_lossy().to_string();
    let size = bundle_size(path);
    let run_id = derive_run_id(&source_path, size);

    let mut spectra: Vec<Spectrum> = Vec::with_capacity(frames.len());
    let mut ms1_count: u32 = 0;
    let mut ms2_count: u32 = 0;
    let mut scan_counter: u32 = 0;

    for frame in &frames {
        let peaks = reader
            .decode_peaks(frame)
            .map_err(|e| MsError::Other(format!("decode frame {}: {}", frame.id, e)))?;

        match frame.msms_type {
            0 => {
                scan_counter += 1;
                spectra.push(build_ms1(&run_id, scan_counter, frame, &peaks, &calibration));
                ms1_count += 1;
            }
            8 => {
                let infos = reader.pasef_msms_info_for_frame(frame.id).map_err(|e| {
                    MsError::Other(format!("pasef info frame {}: {}", frame.id, e))
                })?;
                for info in infos {
                    let prec = reader.precursor(info.precursor_id).map_err(|e| {
                        MsError::Other(format!(
                            "precursor {} (frame {}): {}",
                            info.precursor_id, frame.id, e
                        ))
                    })?;
                    scan_counter += 1;
                    spectra.push(build_pasef_ms2(
                        &run_id,
                        scan_counter,
                        frame,
                        &info,
                        prec.as_ref(),
                        &peaks,
                        &calibration,
                    ));
                    ms2_count += 1;
                }
            }
            9 => {
                let windows = reader.dia_windows_for_frame(frame.id).map_err(|e| {
                    MsError::Other(format!("dia windows frame {}: {}", frame.id, e))
                })?;
                if let Some(group) = windows {
                    for w in &group.windows {
                        scan_counter += 1;
                        spectra.push(build_dia_ms2(
                            &run_id,
                            scan_counter,
                            frame,
                            w,
                            &peaks,
                            &calibration,
                        ));
                        ms2_count += 1;
                    }
                }
            }
            _ => {
                // Unknown msms_type (PRM-PASEF = 10, etc.). Skip for now.
                continue;
            }
        }
    }

    let run = Run {
        run_id,
        source_path: Some(source_path),
        source_format: "bruker-tdf".into(),
        instrument: Some(metadata.instrument_name.clone()),
        start_time: None,
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

fn build_ms1(
    run_id: &str,
    scan_num: u32,
    frame: &Frame,
    peaks: &[Peak],
    cal: &Calibration,
) -> Spectrum {
    let pa = materialize_peaks(peaks, cal, None, None);

    Spectrum {
        run_id: run_id.to_string(),
        scan_num,
        native_id: Some(format!("frame={} scan=1", frame.id)),
        ms_level: 1,
        rt: Some(frame.time),
        tic: Some(pa.tic),
        base_peak_mz: pa.base_peak_mz,
        base_peak_intensity: pa.base_peak_intensity,
        polarity: polarity_for(frame),
        centroided: Some(true),
        precursor: None,
        activation: None,
        collision_energy: None,
        inv_mobility: pa.inv_mobility,
        mz_precision: Some(64),
        intensity_precision: Some(32),
        scan_window_lower: pa.scan_window_lower,
        scan_window_upper: pa.scan_window_upper,
        mz: pa.mz,
        intensity: pa.intensity,
        cv_params: None,
    }
}

fn build_pasef_ms2(
    run_id: &str,
    scan_num: u32,
    frame: &Frame,
    info: &PasefMsMsInfo,
    tdf_prec: Option<&TdfPrecursor>,
    peaks: &[Peak],
    cal: &Calibration,
) -> Spectrum {
    let pa = materialize_peaks(
        peaks,
        cal,
        Some(info.scan_num_begin),
        Some(info.scan_num_end),
    );

    let prec_mz = tdf_prec
        .and_then(|p| p.monoisotopic_mz)
        .or_else(|| tdf_prec.map(|p| p.largest_peak_mz));
    let half = info.isolation_width / 2.0;
    let precursor = Some(Precursor {
        mz: prec_mz,
        charge: tdf_prec.and_then(|p| p.charge).map(|c| c as i8),
        intensity: tdf_prec.map(|p| p.intensity),
        isolation_window_target: Some(info.isolation_mz),
        isolation_window_lower: Some(half),
        isolation_window_upper: Some(half),
    });

    Spectrum {
        run_id: run_id.to_string(),
        scan_num,
        native_id: Some(format!(
            "frame={} scan={}-{}",
            frame.id, info.scan_num_begin, info.scan_num_end
        )),
        ms_level: 2,
        rt: Some(frame.time),
        tic: Some(pa.tic),
        base_peak_mz: pa.base_peak_mz,
        base_peak_intensity: pa.base_peak_intensity,
        polarity: polarity_for(frame),
        centroided: Some(true),
        precursor,
        activation: Some("beam-type collision-induced dissociation".into()),
        collision_energy: Some(info.collision_energy as f32),
        inv_mobility: pa.inv_mobility,
        mz_precision: Some(64),
        intensity_precision: Some(32),
        scan_window_lower: pa.scan_window_lower,
        scan_window_upper: pa.scan_window_upper,
        mz: pa.mz,
        intensity: pa.intensity,
        cv_params: None,
    }
}

fn build_dia_ms2(
    run_id: &str,
    scan_num: u32,
    frame: &Frame,
    window: &DiaWindow,
    peaks: &[Peak],
    cal: &Calibration,
) -> Spectrum {
    let pa = materialize_peaks(
        peaks,
        cal,
        Some(window.scan_num_begin),
        Some(window.scan_num_end),
    );

    let half = window.isolation_width / 2.0;
    let precursor = Some(Precursor {
        mz: Some(window.isolation_mz),
        charge: None,
        intensity: None,
        isolation_window_target: Some(window.isolation_mz),
        isolation_window_lower: Some(half),
        isolation_window_upper: Some(half),
    });

    Spectrum {
        run_id: run_id.to_string(),
        scan_num,
        native_id: Some(format!(
            "frame={} scan={}-{}",
            frame.id, window.scan_num_begin, window.scan_num_end
        )),
        ms_level: 2,
        rt: Some(frame.time),
        tic: Some(pa.tic),
        base_peak_mz: pa.base_peak_mz,
        base_peak_intensity: pa.base_peak_intensity,
        polarity: polarity_for(frame),
        centroided: Some(true),
        precursor,
        activation: Some("beam-type collision-induced dissociation".into()),
        collision_energy: Some(window.collision_energy as f32),
        inv_mobility: pa.inv_mobility,
        mz_precision: Some(64),
        intensity_precision: Some(32),
        scan_window_lower: pa.scan_window_lower,
        scan_window_upper: pa.scan_window_upper,
        mz: pa.mz,
        intensity: pa.intensity,
        cv_params: None,
    }
}

/// Output of [`materialize_peaks`].
struct PeakArrays {
    mz: Vec<f64>,
    intensity: Vec<f32>,
    tic: f64,
    base_peak_mz: Option<f64>,
    base_peak_intensity: Option<f64>,
    scan_window_lower: Option<f64>,
    scan_window_upper: Option<f64>,
    inv_mobility: Option<f64>,
}

/// Project a slice of decoded peaks into the column-oriented arrays the rest
/// of ProLance expects, optionally filtered by mobility scan range
/// `[scan_lo, scan_hi)` (used for PASEF / diaPASEF sub-windows).
fn materialize_peaks(
    peaks: &[Peak],
    cal: &Calibration,
    scan_lo: Option<u32>,
    scan_hi: Option<u32>,
) -> PeakArrays {
    let mut filtered: Vec<(f64, f32, u32)> = Vec::new();
    for p in peaks {
        if let Some(lo) = scan_lo {
            if p.scan < lo {
                continue;
            }
        }
        if let Some(hi) = scan_hi {
            if p.scan >= hi {
                continue;
            }
        }
        let mz = cal.tof_to_mz(p.tof);
        filtered.push((mz, p.intensity as f32, p.scan));
    }
    if filtered.is_empty() {
        return PeakArrays {
            mz: Vec::new(),
            intensity: Vec::new(),
            tic: 0.0,
            base_peak_mz: None,
            base_peak_intensity: None,
            scan_window_lower: None,
            scan_window_upper: None,
            inv_mobility: None,
        };
    }
    filtered.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut mz = Vec::with_capacity(filtered.len());
    let mut intensity = Vec::with_capacity(filtered.len());
    let mut tic: f64 = 0.0;
    let mut bp_mz: f64 = filtered[0].0;
    let mut bp_int: f32 = 0.0;
    let mut scan_sum: u64 = 0;
    for (m, i, s) in &filtered {
        mz.push(*m);
        intensity.push(*i);
        tic += *i as f64;
        if *i > bp_int {
            bp_int = *i;
            bp_mz = *m;
        }
        scan_sum += *s as u64;
    }
    let mean_scan = scan_sum as f64 / filtered.len() as f64;
    let inv_mob = cal.scan_to_inv_mobility(mean_scan.round() as u32);

    let lo = filtered.first().map(|t| t.0);
    let hi = filtered.last().map(|t| t.0);

    PeakArrays {
        mz,
        intensity,
        tic,
        base_peak_mz: Some(bp_mz),
        base_peak_intensity: Some(bp_int as f64),
        scan_window_lower: lo,
        scan_window_upper: hi,
        inv_mobility: Some(inv_mob),
    }
}

/// `MzCalibration.Id == 1` corresponds to positive polarity, `2` to negative.
/// See `opentimstdf` SPEC §5.
fn polarity_for(frame: &Frame) -> Option<i8> {
    match frame.mz_calibration_id {
        1 => Some(1),
        2 => Some(-1),
        _ => None,
    }
}

/// Combined size of the two main bundle files (`analysis.tdf` +
/// `analysis.tdf_bin`). Used only to seed the run_id hash.
fn bundle_size(dir: &Path) -> u64 {
    let mut total: u64 = 0;
    for name in ["analysis.tdf", "analysis.tdf_bin"] {
        if let Ok(meta) = std::fs::metadata(dir.join(name)) {
            total += meta.len();
        }
    }
    total
}

/// Stable run identifier (mirrors thermo / mzml ingest paths).
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
