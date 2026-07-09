//! Thermo .raw -> MzmlData via the unified vendor ingest (no mzML round trip).

#![cfg(feature = "vendors")]

use speclance_ms::vendor;

#[test]
fn thermo_ingest_small_raw() {
    let path = "/workspaces/Projects/OpenTFRaw/samples/small.RAW";
    if std::fs::metadata(path).is_err() {
        eprintln!("skip: {path} not present");
        return;
    }
    let data = vendor::ingest(path).expect("thermo ingest");
    assert!(!data.spectra.is_empty(), "expected at least one spectrum");
    assert!(!data.run.run_id.is_empty());
    assert_eq!(data.run.source_format, "thermo-raw");
    assert_eq!(
        data.run.spectrum_count,
        Some(data.spectra.len() as u32),
        "spectrum_count must match collected spectra"
    );
    let ms1 = data.run.ms1_count.unwrap_or(0);
    let ms2 = data.run.ms2_count.unwrap_or(0);
    assert!(ms1 > 0, "expected at least one MS1 in small.RAW");
    assert!(ms2 > 0, "expected at least one MS2 in small.RAW");
    assert_eq!(ms1 + ms2, data.spectra.len() as u32);

    for s in &data.spectra {
        assert_eq!(s.mz.len(), s.intensity.len(), "mz/intensity len mismatch");
        // Every spectrum should have an RT, polarity, and centroid flag set
        // by the direct path; the mzML path also guarantees these on Thermo.
        assert!(s.rt.is_some(), "spectrum {} missing rt", s.scan_num);
        assert!(
            s.polarity.is_some(),
            "spectrum {} missing polarity",
            s.scan_num
        );
        assert!(
            s.native_id.as_deref().unwrap_or("").contains("scan="),
            "native_id should be Thermo controller id"
        );
        if s.ms_level >= 2 {
            assert!(s.precursor.is_some(), "MS{} missing precursor", s.ms_level);
        }
    }
    eprintln!(
        "thermo small.RAW: {} spectra ({} MS1, {} MS2)",
        data.spectra.len(),
        ms1,
        ms2
    );
}
