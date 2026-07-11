//! End-to-end: read mzML -> Lance store -> read back -> emit mzML.

use speclance_core::{batches_to_chromatograms, batches_to_runs, batches_to_spectra, Store};
use speclance_ms::mzml::{parse_bytes, read_mzml, write_mzml};

#[tokio::test]
async fn store_roundtrip_real_thermo_if_present() {
    let path = "/workspaces/SpecLance/corpus/thermo/mzml/PXD069101_TSQ_Altis_milla0255_01.mzML";
    if std::fs::metadata(path).is_err() {
        eprintln!("skip: corpus not present");
        return;
    }

    let original = read_mzml(path).expect("read source");

    let tmp = tempfile::tempdir().unwrap();
    let store_dir = tmp.path().to_str().unwrap();
    let store = Store::open(store_dir).await.unwrap();
    store.append_run(&original.run).await.unwrap();
    for chunk in original.spectra.chunks(1024) {
        store.append_spectra(chunk).await.unwrap();
    }
    store
        .append_chromatograms(&original.chromatograms)
        .await
        .unwrap();

    // Read back.
    let run_batch = store
        .read_run(&original.run.run_id)
        .await
        .unwrap()
        .expect("run row");
    let runs = batches_to_runs(std::slice::from_ref(&run_batch));
    let run = runs.into_iter().next().unwrap();
    let spec_batches = store.read_spectra(&original.run.run_id).await.unwrap();
    let spectra = batches_to_spectra(&spec_batches);
    let chrom_batches = store
        .read_chromatograms(&original.run.run_id)
        .await
        .unwrap();
    let chromatograms = batches_to_chromatograms(&chrom_batches);

    assert_eq!(spectra.len(), original.spectra.len());
    // Chromatograms survive the Lance roundtrip; they are dropped by the
    // current openmassspec-core-backed mzML writer (spectrum-only).
    assert_eq!(chromatograms.len(), original.chromatograms.len());

    // Emit mzML, re-parse, compare key fields.
    let out_path = tmp.path().join("out.mzML");
    let mut f = std::fs::File::create(&out_path).unwrap();
    write_mzml(&mut f, &run, &spectra, &chromatograms).unwrap();
    drop(f);

    let bytes = std::fs::read(&out_path).unwrap();
    let reread = parse_bytes(&bytes, out_path.to_string_lossy().to_string()).unwrap();
    assert_eq!(reread.spectra.len(), original.spectra.len());
    // Writer (openmassspec-core) does not currently emit chromatograms;
    // expect 0 on the re-read side. Track upstream for re-enable.
    assert_eq!(reread.chromatograms.len(), 0);

    for i in 0..original.spectra.len() {
        let a = &original.spectra[i];
        let b = &reread.spectra[i];
        assert_eq!(a.ms_level, b.ms_level);
        assert_eq!(a.mz.len(), b.mz.len(), "mz len mismatch at {}", i);
        assert_eq!(
            a.intensity.len(),
            b.intensity.len(),
            "int len mismatch at {}",
            i
        );
        if let (Some(ra), Some(rb)) = (a.rt, b.rt) {
            // Writer formats RT as minutes with 6 decimals (~60us in seconds).
            assert!((ra - rb).abs() < 1e-3, "rt mismatch at {}", i);
        }
    }
}
