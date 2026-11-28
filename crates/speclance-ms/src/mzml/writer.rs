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
    Check, CollisionEnergyUnit, CvTerm, Leniency, MobilityArrayKind, Polarity, PrecursorInfo,
    RunMetadata, ScanMode, SpectrumRecord, TimeAxis, WriteOptions,
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
    let mut metadata = run_to_metadata(run);
    // No spectrum has a retention time: the run has no time axis, which says
    // so instead of writing every scan at 0 s. Some with and some without is
    // refused by the writer, naming the first spectrum that lacks one.
    if !spectra.is_empty() && spectra.iter().all(|s| s.rt.is_none()) {
        metadata.time_axis = TimeAxis::Absent;
    }
    let records: Vec<SpectrumRecord> = spectra
        .iter()
        .enumerate()
        .map(|(idx, s)| spectrum_to_record(idx, s))
        .collect::<MsResult<_>>()?;

    // A stored `centroided` of NULL is an unknown scan mode (a SCIEX run, for
    // one): the file goes out with neither the profile nor the centroid term
    // and a warning userParam on each such spectrum, instead of being refused
    // or labelled with a guess. Every other check stays strict.
    let options =
        WriteOptions::new().with_leniency(Leniency::strict().with(Check::UnknownScanMode));
    let mut src = VecSource::new(metadata, records);
    openmassspec_io::core::write_mzml_with_options(&mut src, out, &options)
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

    let mut meta = RunMetadata::new(
        source_file_name,
        source_file_format,
        native_id_format,
        instrument_term(run.instrument.as_deref()),
        "SpecLance",
        env!("CARGO_PKG_VERSION"),
    );
    meta.start_timestamp = run.start_time.clone();
    meta.mobility_array_kind = Some(MobilityArrayKind::InverseReducedVsPerCm2);
    meta
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

fn spectrum_to_record(idx: usize, s: &Spectrum) -> MsResult<SpectrumRecord> {
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

    if s.precursor.is_none() {
        if let Some(name) = s.activation.as_deref() {
            return Err(MsError::Unsupported(format!(
                "spectrum {idx} (scan {}): activation {name:?} has no precursor to attach to; \
                 refusing to write the spectrum without it",
                s.scan_num
            )));
        }
    }
    let precursor = s
        .precursor
        .as_ref()
        .map(|p| precursor_to_info(idx, p, s))
        .transpose()?;

    // SpecLance has no FAIMS ingest path on any vendor adapter today, so
    // `faims_cv`, `analyzer`, `filter`, `acquisition_event_id`,
    // `ion_injection_time_ms` and `inv_mobility_per_peak` stay `None`.
    let mut rec = SpectrumRecord::new(idx, s.scan_num, native_id)
        .with_peaks(s.mz.clone(), s.intensity.clone());
    rec.ms_level = s.ms_level as u32;
    rec.polarity = polarity;
    rec.scan_mode = scan_mode;
    rec.retention_time_sec = s.rt;
    rec.total_ion_current = s.tic;
    rec.base_peak_mz = s.base_peak_mz;
    rec.base_peak_intensity = s.base_peak_intensity;
    // `scan_window_*` is the instrument's acquisition range, which Core
    // writes as `<scanWindowList>`; the observed range (`low_mz`, `high_mz`)
    // comes from the peaks.
    match (s.scan_window_lower, s.scan_window_upper) {
        (Some(lower), Some(upper)) => rec = rec.with_scan_window(lower, upper),
        (None, None) => {}
        (lower, upper) => {
            return Err(MsError::Unsupported(format!(
                "spectrum {idx} (scan {}): the scan window has only one limit \
                 (lower {lower:?}, upper {upper:?}); refusing to write half a window",
                s.scan_num
            )))
        }
    }
    rec.inv_mobility = s.inv_mobility;
    rec.precursor = precursor;
    Ok(rec)
}

fn precursor_to_info(
    idx: usize,
    p: &speclance_core::Precursor,
    s: &Spectrum,
) -> MsResult<PrecursorInfo> {
    // SpecLance stores symmetric lower/upper half-widths around the target.
    // Restore the full width when both are present.
    let isolation_width = match (p.isolation_window_lower, p.isolation_window_upper) {
        (Some(lo), Some(hi)) => Some(lo + hi),
        _ => None,
    };
    let activation = match s.activation.as_deref() {
        Some(name) => Some(crate::activation::parse_activation(name).ok_or_else(|| {
            MsError::Unsupported(format!(
                "spectrum {idx} (scan {}): activation {name:?} has no mzML activation term; \
                 refusing to write the spectrum without it",
                s.scan_num
            ))
        })?),
        None => None,
    };
    // `PD` has no PSI-MS term. Core writes it as an `openmassspec: activation
    // pd` userParam in `<activation>`, so it is exported, not refused.
    let mut info = PrecursorInfo::default();
    info.target_mz = p.isolation_window_target;
    info.selected_mz = p.mz;
    info.isolation_width = isolation_width;
    info.charge = p.charge.map(|z| z as i32);
    info.intensity = p.intensity;
    // The column holds electronvolts.
    info = info.with_collision_energy(s.collision_energy.map(f64::from), CollisionEnergyUnit::Ev);
    info.activation = activation;
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use speclance_core::Precursor;

    fn ms2(activation: Option<&str>) -> Spectrum {
        Spectrum {
            scan_num: 7,
            ms_level: 2,
            precursor: Some(Precursor {
                mz: Some(500.0),
                ..Default::default()
            }),
            activation: activation.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn known_activation_is_written() {
        let rec =
            spectrum_to_record(0, &ms2(Some("beam-type collision-induced dissociation"))).unwrap();
        assert_eq!(
            rec.precursor.unwrap().activation,
            Some(openmassspec_io::core::Activation::HCD)
        );
    }

    #[test]
    fn unrecognised_activation_is_an_error() {
        let err = spectrum_to_record(3, &ms2(Some("flux capacitor"))).unwrap_err();
        let msg = err.to_string();
        assert!(matches!(err, MsError::Unsupported(_)));
        assert!(
            msg.contains("flux capacitor") && msg.contains("spectrum 3"),
            "{msg}"
        );
    }

    #[test]
    fn write_mzml_rejects_unrecognised_activation() {
        let run = Run::default();
        let mut buf = Vec::new();
        let r = write_mzml(&mut buf, &run, &[ms2(Some("flux capacitor"))], &[]);
        assert!(matches!(r, Err(MsError::Unsupported(_))));
    }

    #[test]
    fn activation_without_precursor_is_an_error() {
        let mut sp = ms2(Some("HCD"));
        sp.precursor = None;
        let err = spectrum_to_record(5, &sp).unwrap_err();
        let msg = err.to_string();
        assert!(matches!(err, MsError::Unsupported(_)));
        assert!(msg.contains("HCD") && msg.contains("spectrum 5"), "{msg}");
    }

    #[test]
    fn pd_is_written_as_the_core_userparam() {
        let rec = spectrum_to_record(2, &ms2(Some("PD"))).unwrap();
        assert_eq!(
            rec.precursor.unwrap().activation,
            Some(openmassspec_io::core::Activation::PD)
        );
        let run = Run::default();
        let mut sp = ms2(Some("PD"));
        sp.rt = Some(1.0);
        sp.mz = vec![100.0];
        sp.intensity = vec![1.0];
        let mut buf = Vec::new();
        write_mzml(&mut buf, &run, &[sp], &[]).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(
            text.contains(r#"name="openmassspec: activation" value="pd""#),
            "{text}"
        );
    }

    fn with_peaks(mut s: Spectrum, rt: Option<f64>) -> Spectrum {
        s.rt = rt;
        s.mz = vec![100.0, 200.0];
        s.intensity = vec![1.0, 2.0];
        s
    }

    #[test]
    fn an_unknown_scan_mode_is_written_unlabelled_with_a_warning() {
        let mut sp = with_peaks(ms2(None), Some(1.0));
        sp.centroided = None;
        let mut buf = Vec::new();
        write_mzml(&mut buf, &Run::default(), &[sp], &[]).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(!text.contains(r#"accession="MS:1000127""#));
        assert!(!text.contains(r#"accession="MS:1000128""#));
        assert!(
            text.contains("openmassspec: warning unknown_scan_mode"),
            "{text}"
        );
    }

    #[test]
    fn a_missing_retention_time_is_not_written_as_zero() {
        let rec = spectrum_to_record(0, &ms2(None)).unwrap();
        assert_eq!(rec.retention_time_sec, None);
        let run = Run::default();

        // No spectrum has one: the run says it has no time axis and no scan
        // start time is written.
        let mut buf = Vec::new();
        write_mzml(&mut buf, &run, &[with_peaks(ms2(None), None)], &[]).unwrap();
        let text = String::from_utf8(buf).unwrap();
        assert!(
            !text.contains("MS:1000016"),
            "scan start time written: {text}"
        );

        // One spectrum has one and one does not: refused, not written as 0 s.
        let mixed = [
            with_peaks(ms2(None), Some(5.0)),
            with_peaks(ms2(None), None),
        ];
        let err = write_mzml(&mut Vec::new(), &run, &mixed, &[]).unwrap_err();
        assert!(err.to_string().contains("retention"), "{err}");
    }

    #[test]
    fn the_scan_window_is_the_acquisition_range_not_the_observed_one() {
        let mut sp = with_peaks(ms2(None), Some(1.0));
        sp.scan_window_lower = Some(50.0);
        sp.scan_window_upper = Some(1500.0);
        let rec = spectrum_to_record(0, &sp).unwrap();
        assert_eq!(rec.scan_windows.len(), 1);
        assert_eq!(
            (rec.scan_windows[0].lower, rec.scan_windows[0].upper),
            (50.0, 1500.0)
        );
        assert_eq!((rec.low_mz, rec.high_mz), (None, None));

        sp.scan_window_upper = None;
        let err = spectrum_to_record(4, &sp).unwrap_err();
        assert!(matches!(err, MsError::Unsupported(_)), "{err:?}");
        assert!(err.to_string().contains("only one limit"), "{err}");
    }

    #[test]
    fn the_collision_energy_column_is_written_in_electronvolts() {
        let mut sp = ms2(None);
        sp.collision_energy = Some(35.0);
        let p = spectrum_to_record(0, &sp).unwrap().precursor.unwrap();
        assert_eq!(p.collision_energies.len(), 1);
        assert_eq!(p.collision_energies[0].value, 35.0);
        assert_eq!(p.collision_energies[0].unit, Some(CollisionEnergyUnit::Ev));
        sp.collision_energy = None;
        let p = spectrum_to_record(0, &sp).unwrap().precursor.unwrap();
        assert!(p.collision_energies.is_empty());
    }

    #[test]
    fn absent_activation_is_not_an_error() {
        assert!(spectrum_to_record(0, &ms2(None)).is_ok());
    }
}
