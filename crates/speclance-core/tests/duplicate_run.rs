//! Re-ingesting a run that is already in the store must fail cleanly
//! instead of duplicating rows.

use speclance_core::{Error, Run, Spectrum, Store};

fn run(id: &str) -> Run {
    Run {
        run_id: id.to_string(),
        source_format: "mzml".to_string(),
        ..Default::default()
    }
}

fn spectrum(run_id: &str, scan: u32) -> Spectrum {
    Spectrum {
        run_id: run_id.to_string(),
        scan_num: scan,
        ms_level: 1,
        rt: Some(scan as f64),
        mz: vec![100.0],
        intensity: vec![1.0],
        ..Default::default()
    }
}

#[tokio::test]
async fn append_run_rejects_existing_run_id() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();

    assert!(!store.contains_run("r1").await.unwrap());
    store.ensure_run_absent("r1").await.unwrap();
    store.append_run(&run("r1")).await.unwrap();
    assert!(store.contains_run("r1").await.unwrap());

    let err = store.append_run(&run("r1")).await.unwrap_err();
    assert!(
        matches!(err, Error::RunExists(ref id) if id == "r1"),
        "{err}"
    );
    let err = store.ensure_run_absent("r1").await.unwrap_err();
    assert!(matches!(err, Error::RunExists(_)), "{err}");

    // A different run is still accepted.
    store.append_run(&run("r2")).await.unwrap();
    let rows = store
        .runs_table()
        .await
        .unwrap()
        .count_rows(None)
        .await
        .unwrap();
    assert_eq!(rows, 2);
}

#[tokio::test]
async fn ensure_run_absent_sees_orphan_spectra() {
    // Streaming ingest writes spectra before the run row; an interrupted
    // ingest leaves spectra with no run. That must still block a re-ingest.
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();
    store
        .append_spectra(&[spectrum("r1", 1), spectrum("r1", 2)])
        .await
        .unwrap();
    let err = store.ensure_run_absent("r1").await.unwrap_err();
    assert!(matches!(err, Error::RunExists(_)), "{err}");
    store.ensure_run_absent("r2").await.unwrap();
}
