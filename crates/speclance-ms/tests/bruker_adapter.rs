//! Bruker timsTOF `.d/` -> MzmlData via the unified vendor ingest.

#![cfg(feature = "vendors")]

use speclance_ms::vendor;

#[test]
fn bruker_ingest_smoke() {
    // Pick the first available .d/ bundle from the corpus we ship to CI.
    // PXD028279 is a small (29 MB) PASEF DDA bundle with both MS1 and MS2
    // frames - it exercises the MS1 pooling path and the per-PASEF-row MS2
    // emission path.
    let candidates = [
        // OpenTimsTDF's own validation corpus is the most reliable source of well-
        // formed `.d/` bundles. We pick the smallest available (~106 MB).
        "/workspaces/Projects/OpenTimsTDF/re/artifacts/cache/pride/PXD036417/NQO1-F107C_coi-N2-P_200-0C_3996.d",
        "/workspaces/Projects/OpenTimsTDF/re/artifacts/cache/pride/PXD031833/mTTYH1-D_coi-N2-P-200-20C_U-T_3366.d",
    ];
    let Some(path) = candidates
        .iter()
        .find(|p| std::path::Path::new(p).join("analysis.tdf").is_file())
    else {
        eprintln!("skip: no bruker .d/ bundle available");
        return;
    };

    let data = vendor::ingest(path).expect("bruker ingest");
    assert!(!data.spectra.is_empty(), "expected at least one spectrum");
    assert_eq!(data.run.source_format, "bruker-tdf");
    assert!(!data.run.run_id.is_empty());
    assert_eq!(
        data.run.spectrum_count,
        Some(data.spectra.len() as u32),
        "spectrum_count must match collected spectra"
    );
    let ms1 = data.run.ms1_count.unwrap_or(0);
    let ms2 = data.run.ms2_count.unwrap_or(0);
    assert_eq!(ms1 + ms2, data.spectra.len() as u32);
    assert!(ms1 > 0, "expected at least one MS1 frame");

    for s in &data.spectra {
        assert_eq!(s.mz.len(), s.intensity.len(), "mz/intensity len mismatch");
        if !s.mz.is_empty() {
            // Sorted m/z invariant.
            for w in s.mz.windows(2) {
                assert!(w[0] <= w[1], "mz must be sorted ascending");
            }
            assert!(s.rt.is_some());
        }
        if s.ms_level >= 2 {
            assert!(s.precursor.is_some(), "MS{} missing precursor", s.ms_level);
        }
    }
    eprintln!(
        "bruker {}: {} spectra ({} MS1, {} MS2)",
        path,
        data.spectra.len(),
        ms1,
        ms2
    );
}
