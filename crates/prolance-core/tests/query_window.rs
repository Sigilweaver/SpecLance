//! Range-query and scalar-index tests for `Store::query_window`.

use prolance_core::{batches_to_spectra, Spectrum, Store};

fn synth_spectrum(run_id: &str, scan: u32, ms_level: u8, rt: f64) -> Spectrum {
    Spectrum {
        run_id: run_id.to_string(),
        scan_num: scan,
        ms_level,
        rt: Some(rt),
        mz: vec![100.0, 200.0, 300.0],
        intensity: vec![1.0, 2.0, 3.0],
        tic: Some(6.0),
        ..Default::default()
    }
}

#[tokio::test]
async fn query_window_filters_by_rt_and_ms_level() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();

    let run_id = "test-run-1";
    let mut spectra = Vec::new();
    for i in 0..20u32 {
        let lvl = if i % 2 == 0 { 1u8 } else { 2u8 };
        spectra.push(synth_spectrum(run_id, i, lvl, i as f64 * 10.0));
    }
    // Add a second run to make sure run_id filtering is honoured.
    spectra.push(synth_spectrum("other-run", 0, 1, 50.0));

    store.append_spectra(&spectra).await.unwrap();

    // Full range, no level constraint.
    let all = store
        .query_window(run_id, f64::NEG_INFINITY, f64::INFINITY, None)
        .await
        .unwrap();
    let all_specs = batches_to_spectra(&all);
    assert_eq!(all_specs.len(), 20);
    assert!(all_specs.iter().all(|s| s.run_id == run_id));

    // RT window [50, 100] -> scans 5..=10, that's 6 spectra.
    let win = store
        .query_window(run_id, 50.0, 100.0, None)
        .await
        .unwrap();
    let win_specs = batches_to_spectra(&win);
    assert_eq!(win_specs.len(), 6);
    assert!(win_specs.iter().all(|s| s.rt.unwrap() >= 50.0 && s.rt.unwrap() <= 100.0));

    // MS1 only across full range -> 10 spectra.
    let ms1 = store
        .query_window(run_id, f64::NEG_INFINITY, f64::INFINITY, Some(1))
        .await
        .unwrap();
    let ms1_specs = batches_to_spectra(&ms1);
    assert_eq!(ms1_specs.len(), 10);
    assert!(ms1_specs.iter().all(|s| s.ms_level == 1));

    // RT window [30, 70] + MS2 -> odd scans 3, 5, 7 -> 3 spectra.
    let ms2 = store.query_window(run_id, 30.0, 70.0, Some(2)).await.unwrap();
    let ms2_specs = batches_to_spectra(&ms2);
    assert_eq!(ms2_specs.len(), 3);
    assert!(ms2_specs
        .iter()
        .all(|s| s.ms_level == 2 && s.rt.unwrap() >= 30.0 && s.rt.unwrap() <= 70.0));
}

#[tokio::test]
async fn create_default_indexes_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();

    // Seed enough rows for indexing to be meaningful (BTree/Bitmap
    // builders are happy with small inputs, but seeding > 1 row
    // exercises the full path).
    let mut spectra = Vec::new();
    for i in 0..32u32 {
        spectra.push(synth_spectrum("idx-run", i, (i % 2) as u8 + 1, i as f64));
    }
    store.append_spectra(&spectra).await.unwrap();

    store.create_default_indexes().await.unwrap();
    // Second call should not error; lancedb replaces the existing
    // index in place.
    store.create_default_indexes().await.unwrap();

    // Indexed query path still returns the same rows.
    let hits = store
        .query_window("idx-run", 5.0, 10.0, Some(1))
        .await
        .unwrap();
    let specs = batches_to_spectra(&hits);
    assert!(specs.iter().all(|s| s.ms_level == 1 && s.rt.unwrap() >= 5.0 && s.rt.unwrap() <= 10.0));
}
