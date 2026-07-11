//! Waters `.raw/` -> MzmlData via the unified vendor ingest.

#![cfg(feature = "vendors")]

use speclance_ms::vendor;

#[test]
fn waters_ingest_smoke() {
    let Some(path) = std::env::var("OPENMASSSPEC_WATERS_RAW").ok().filter(|p| {
        std::path::Path::new(p).join("_FUNCTNS.INF").is_file()
            || std::path::Path::new(p).join("_extern.inf").is_file()
            || std::path::Path::new(p).is_dir()
    }) else {
        eprintln!("skip: OPENMASSSPEC_WATERS_RAW not set");
        return;
    };

    let data = vendor::ingest(&path).expect("waters ingest");
    assert!(!data.spectra.is_empty(), "expected at least one spectrum");
    assert_eq!(data.run.source_format, "waters-raw");
    assert!(!data.run.run_id.is_empty());
    assert_eq!(
        data.run.spectrum_count,
        Some(data.spectra.len() as u32),
        "spectrum_count must match collected spectra"
    );
    for s in &data.spectra {
        assert_eq!(s.mz.len(), s.intensity.len(), "mz/intensity len mismatch");
    }
}
