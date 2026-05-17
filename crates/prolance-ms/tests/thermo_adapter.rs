//! Thermo .raw -> MzmlData via opentfraw -> our mzML reader.

#![cfg(feature = "thermo")]

use prolance_ms::thermo;

#[test]
fn thermo_ingest_small_raw() {
    let path = "/workspaces/Projects/OpenTFRaw/samples/small.RAW";
    if std::fs::metadata(path).is_err() {
        eprintln!("skip: {path} not present");
        return;
    }
    let data = thermo::ingest(path).expect("thermo ingest");
    assert!(!data.spectra.is_empty(), "expected at least one spectrum");
    // Run metadata should be populated.
    assert!(!data.run.run_id.is_empty());
    // Check that mz/intensity arrays match in size per spectrum.
    for s in &data.spectra {
        assert_eq!(s.mz.len(), s.intensity.len(), "mz/intensity len mismatch");
    }
    eprintln!(
        "thermo small.RAW: {} spectra, {} chromatograms",
        data.spectra.len(),
        data.chromatograms.len()
    );
}
