//! Re-ingesting a run that is already in the store must fail cleanly
//! instead of duplicating rows.

use speclance_core::{Chromatogram, Error, Run, Spectrum, Store};

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
async fn append_run_rejects_existing_id_among_many_fragments() {
    // Each append is its own Lance fragment; the guard has to look at the
    // whole table, not at the first batch a scan returns.
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();
    for id in ["a", "b", "c", "d"] {
        store.append_run(&run(id)).await.unwrap();
    }
    for id in ["a", "b", "c", "d"] {
        let err = store.append_run(&run(id)).await.unwrap_err();
        assert!(matches!(err, Error::RunExists(ref r) if r == id), "{err}");
    }
    store.append_run(&run("e")).await.unwrap();
    let rows = store
        .runs_table()
        .await
        .unwrap()
        .count_rows(None)
        .await
        .unwrap();
    assert_eq!(rows, 5);
}

#[tokio::test]
async fn spectra_without_a_run_row_do_not_make_the_run_exist() {
    // Ingest writes the run row last, so spectra alone are an unfinished
    // ingest, not a run.
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();
    store
        .append_spectra(&[spectrum("r1", 1), spectrum("r1", 2)])
        .await
        .unwrap();
    assert!(!store.contains_run("r1").await.unwrap());
    store.ensure_run_absent("r1").await.unwrap();
}

#[tokio::test]
async fn discard_unfinished_run_removes_only_that_runs_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();

    // Nothing written yet: no tables, nothing to remove.
    assert_eq!(store.discard_unfinished_run("r1").await.unwrap(), 0);

    store
        .append_spectra(&[spectrum("r1", 1), spectrum("r1", 2), spectrum("r2", 1)])
        .await
        .unwrap();
    store
        .append_chromatograms(&[Chromatogram {
            run_id: "r1".to_string(),
            chrom_id: "TIC".to_string(),
            ..Default::default()
        }])
        .await
        .unwrap();

    assert_eq!(store.discard_unfinished_run("r1").await.unwrap(), 3);
    assert_eq!(
        store
            .read_spectra("r1")
            .await
            .unwrap()
            .iter()
            .map(|b| b.num_rows())
            .sum::<usize>(),
        0
    );
    assert!(store
        .read_chromatograms("r1")
        .await
        .unwrap()
        .iter()
        .all(|b| b.num_rows() == 0));
    // Another run's rows stay, and a second call has nothing left to remove.
    let other: usize = store
        .read_spectra("r2")
        .await
        .unwrap()
        .iter()
        .map(|b| b.num_rows())
        .sum();
    assert_eq!(other, 1);
    assert_eq!(store.discard_unfinished_run("r1").await.unwrap(), 0);
}

#[tokio::test]
async fn discard_unfinished_run_refuses_a_run_that_has_a_run_row() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();
    store.append_spectra(&[spectrum("r1", 1)]).await.unwrap();
    store.append_run(&run("r1")).await.unwrap();

    let err = store.discard_unfinished_run("r1").await.unwrap_err();
    assert!(matches!(err, Error::RunExists(_)), "{err}");
    let kept: usize = store
        .read_spectra("r1")
        .await
        .unwrap()
        .iter()
        .map(|b| b.num_rows())
        .sum();
    assert_eq!(kept, 1);
}

#[tokio::test]
async fn discard_unfinished_run_handles_a_quote_in_the_run_id() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();
    let tricky = "o'brien";
    store
        .append_spectra(&[spectrum(tricky, 1), spectrum("obrien", 1)])
        .await
        .unwrap();
    assert_eq!(store.discard_unfinished_run(tricky).await.unwrap(), 1);
    let kept: usize = store
        .read_spectra("obrien")
        .await
        .unwrap()
        .iter()
        .map(|b| b.num_rows())
        .sum();
    assert_eq!(kept, 1);
}
