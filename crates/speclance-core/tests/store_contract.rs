//! Store behaviour contract.
//!
//! These tests pin down the observable behaviour of `Store` that depends on
//! the lancedb / Lance / Arrow stack underneath it: the on-disk schemas, the
//! row set and ordering returned by plain and filtered scans, filter
//! escaping, inclusive and open range bounds, null handling, and that the
//! scalar-index query path returns exactly the same rows as the unindexed
//! scan path. They exist so that a storage-stack upgrade that changes any of
//! this fails loudly instead of silently altering query results.

use std::sync::Arc;

use arrow_array::RecordBatch;
use arrow_schema::Schema;
use futures::TryStreamExt;
use lancedb::query::ExecutableQuery;
use speclance_core::schema::{chromatograms_schema, runs_schema, spectra_schema};
use speclance_core::{
    batches_to_chromatograms, batches_to_runs, batches_to_spectra, Chromatogram, Precursor, Run,
    Spectrum, Store,
};

fn synth_spectrum(run_id: &str, scan: u32, ms_level: u8, rt: Option<f64>) -> Spectrum {
    Spectrum {
        run_id: run_id.to_string(),
        scan_num: scan,
        ms_level,
        rt,
        mz: vec![100.0 + scan as f64, 200.0],
        intensity: vec![scan as f32, 1.5],
        tic: Some(1.5 + scan as f64),
        ..Default::default()
    }
}

fn fully_populated_spectrum() -> Spectrum {
    Spectrum {
        run_id: "full".into(),
        scan_num: 42,
        native_id: Some("controllerType=0 controllerNumber=1 scan=42".into()),
        ms_level: 2,
        rt: Some(123.456),
        tic: Some(9.0e6),
        base_peak_mz: Some(445.12),
        base_peak_intensity: Some(1.0e5),
        polarity: Some(-1),
        centroided: Some(true),
        precursor: Some(Precursor {
            mz: Some(445.12),
            charge: Some(2),
            intensity: Some(5.5e4),
            isolation_window_target: Some(445.12),
            isolation_window_lower: Some(0.8),
            isolation_window_upper: Some(0.8),
        }),
        activation: Some("HCD".into()),
        collision_energy: Some(30.0),
        inv_mobility: Some(0.95),
        mz_precision: Some(64),
        intensity_precision: Some(32),
        scan_window_lower: Some(100.0),
        scan_window_upper: Some(1500.0),
        mz: vec![100.5, 200.25, 300.125],
        intensity: vec![10.0, 20.0, 30.0],
        cv_params: Some(r#"{"MS:1000512":"filter string"}"#.into()),
    }
}

/// Serialize records to JSON values so they can be compared field by field
/// (the record types intentionally do not derive `PartialEq`).
fn as_json<T: serde::Serialize>(items: &[T]) -> Vec<serde_json::Value> {
    items
        .iter()
        .map(|i| serde_json::to_value(i).expect("record serializes"))
        .collect()
}

fn scan_nums(specs: &[Spectrum]) -> Vec<u32> {
    specs.iter().map(|s| s.scan_num).collect()
}

/// Compare schemas on the parts that define the stored data: field names,
/// order, types (including nested list item fields) and nullability.
fn assert_same_fields(actual: &Schema, expected: &Schema, table: &str) {
    let a: Vec<_> = actual
        .fields()
        .iter()
        .map(|f| (f.name().clone(), f.data_type().clone(), f.is_nullable()))
        .collect();
    let e: Vec<_> = expected
        .fields()
        .iter()
        .map(|f| (f.name().clone(), f.data_type().clone(), f.is_nullable()))
        .collect();
    assert_eq!(a, e, "on-disk schema drifted for table `{table}`");
}

async fn scan_all(table: &lancedb::Table) -> Vec<RecordBatch> {
    table
        .query()
        .execute()
        .await
        .expect("plain scan")
        .try_collect()
        .await
        .expect("collect plain scan")
}

#[tokio::test]
async fn on_disk_schemas_match_declared_schemas() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();

    store
        .append_run(&Run {
            run_id: "r".into(),
            source_format: "mzml".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    store
        .append_spectra(&[synth_spectrum("r", 1, 1, Some(1.0))])
        .await
        .unwrap();
    store
        .append_chromatograms(&[Chromatogram {
            run_id: "r".into(),
            chrom_id: "TIC".into(),
            time: vec![0.0, 1.0],
            intensity: vec![5.0, 6.0],
            ..Default::default()
        }])
        .await
        .unwrap();

    let declared: [(&str, Arc<Schema>); 3] = [
        ("runs", runs_schema()),
        ("spectra", spectra_schema()),
        ("chromatograms", chromatograms_schema()),
    ];
    for (name, schema) in declared {
        let table = store.conn.open_table(name).execute().await.unwrap();
        let on_disk = table.schema().await.unwrap();
        assert_same_fields(&on_disk, &schema, name);

        // Batches coming back out of a scan carry the same schema.
        let batches = scan_all(&table).await;
        assert!(!batches.is_empty(), "table `{name}` returned no batches");
        for b in &batches {
            assert_same_fields(&b.schema(), &schema, name);
        }
    }
}

#[tokio::test]
async fn empty_tables_have_declared_schemas() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();

    let spectra = store.spectra_table().await.unwrap();
    assert_same_fields(
        &spectra.schema().await.unwrap(),
        &spectra_schema(),
        "spectra",
    );
    assert_eq!(spectra.count_rows(None).await.unwrap(), 0);
    assert!(store.read_spectra("nope").await.unwrap().is_empty());
    assert!(store
        .query_window("nope", f64::NEG_INFINITY, f64::INFINITY, None)
        .await
        .unwrap()
        .is_empty());

    // Without a chromatograms table, reads return empty instead of
    // creating the table.
    assert!(store.read_chromatograms("nope").await.unwrap().is_empty());
    assert!(!store
        .table_names()
        .await
        .unwrap()
        .iter()
        .any(|n| n == "chromatograms"));
}

#[tokio::test]
async fn spectrum_fields_round_trip_exactly() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();

    let full = fully_populated_spectrum();
    // Every optional field left as None, including the precursor and the
    // peak arrays empty.
    let sparse = Spectrum {
        run_id: "full".into(),
        scan_num: 43,
        ms_level: 1,
        ..Default::default()
    };
    let input = vec![full, sparse];
    store.append_spectra(&input).await.unwrap();

    let out = batches_to_spectra(&store.read_spectra("full").await.unwrap());
    // `batches_to_spectra` materialises an all-null precursor as `None`.
    assert_eq!(as_json(&out), as_json(&input));
}

#[tokio::test]
async fn run_and_chromatogram_fields_round_trip_exactly() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();

    let run = Run {
        run_id: "run-1".into(),
        source_path: Some("/data/a.mzML".into()),
        source_format: "mzml".into(),
        instrument: Some("Orbitrap Exploris 480".into()),
        start_time: Some("2026-01-02T03:04:05Z".into()),
        ingested_at: Some("2026-11-05T00:00:00Z".into()),
        spectrum_count: Some(10),
        ms1_count: Some(4),
        ms2_count: Some(6),
        run_metadata: Some(r#"{"k":"v"}"#.into()),
    };
    store.append_run(&run).await.unwrap();
    let got = store.read_run("run-1").await.unwrap().expect("run present");
    assert_eq!(got.num_rows(), 1);
    assert_eq!(
        as_json(&batches_to_runs(std::slice::from_ref(&got))),
        as_json(std::slice::from_ref(&run))
    );
    assert!(store.read_run("run-2").await.unwrap().is_none());

    let chroms = vec![
        Chromatogram {
            run_id: "run-1".into(),
            chrom_id: "TIC".into(),
            chrom_type: Some("total ion current chromatogram".into()),
            precursor_mz: None,
            product_mz: None,
            time: vec![0.0, 0.5, 1.0],
            intensity: vec![1.0, 2.0, 3.0],
            cv_params: None,
        },
        Chromatogram {
            run_id: "run-1".into(),
            chrom_id: "SRM SIC 500.1,600.2".into(),
            chrom_type: Some("selected reaction monitoring chromatogram".into()),
            precursor_mz: Some(500.1),
            product_mz: Some(600.2),
            time: vec![],
            intensity: vec![],
            cv_params: Some("{}".into()),
        },
    ];
    store.append_chromatograms(&chroms).await.unwrap();
    store
        .append_chromatograms(&[Chromatogram {
            run_id: "run-2".into(),
            chrom_id: "TIC".into(),
            ..Default::default()
        }])
        .await
        .unwrap();
    let got = batches_to_chromatograms(&store.read_chromatograms("run-1").await.unwrap());
    assert_eq!(as_json(&got), as_json(&chroms));
}

#[tokio::test]
async fn scans_return_every_row_in_insertion_order() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();

    // Several appends (one Lance fragment each), interleaving two runs, and
    // well past any implicit result limit a query builder might apply.
    let mut expected = Vec::new();
    for chunk in 0..5u32 {
        let mut batch = Vec::new();
        for i in 0..250u32 {
            let scan = chunk * 250 + i;
            let s = synth_spectrum("ordered", scan, 1 + (scan % 2) as u8, Some(scan as f64));
            expected.push(scan);
            batch.push(s);
            batch.push(synth_spectrum("noise", scan, 1, Some(scan as f64)));
        }
        store.append_spectra(&batch).await.unwrap();
    }

    let got = batches_to_spectra(&store.read_spectra("ordered").await.unwrap());
    assert_eq!(scan_nums(&got), expected);

    let all = store
        .query_window("ordered", f64::NEG_INFINITY, f64::INFINITY, None)
        .await
        .unwrap();
    assert_eq!(scan_nums(&batches_to_spectra(&all)), expected);

    // Unfiltered scan of the whole table returns both runs: 2 x 1250 rows.
    let table = store.spectra_table().await.unwrap();
    let rows: usize = scan_all(&table).await.iter().map(|b| b.num_rows()).sum();
    assert_eq!(rows, 2500);
    assert_eq!(table.count_rows(None).await.unwrap(), 2500);
}

#[tokio::test]
async fn query_window_bounds_are_inclusive_and_nulls_follow_bounds() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();

    let spectra = vec![
        synth_spectrum("b", 0, 1, Some(10.0)),
        synth_spectrum("b", 1, 1, Some(20.0)),
        synth_spectrum("b", 2, 2, Some(30.0)),
        synth_spectrum("b", 3, 1, None),
        synth_spectrum("b", 4, 2, Some(10.000_001)),
    ];
    store.append_spectra(&spectra).await.unwrap();

    let q = |lo: f64, hi: f64, lvl: Option<u8>| {
        let store = &store;
        async move {
            scan_nums(&batches_to_spectra(
                &store.query_window("b", lo, hi, lvl).await.unwrap(),
            ))
        }
    };

    // Both bounds are inclusive.
    assert_eq!(q(10.0, 20.0, None).await, vec![0, 1, 4]);
    assert_eq!(q(20.0, 20.0, None).await, vec![1]);
    // Fractional bounds are not truncated by the filter formatting.
    assert_eq!(q(10.0000005, 30.0, None).await, vec![1, 2, 4]);
    // A finite bound excludes spectra without an RT ...
    assert_eq!(q(f64::NEG_INFINITY, 1000.0, None).await, vec![0, 1, 2, 4]);
    assert_eq!(q(0.0, f64::INFINITY, None).await, vec![0, 1, 2, 4]);
    // ... while a fully open window keeps them.
    assert_eq!(
        q(f64::NEG_INFINITY, f64::INFINITY, None).await,
        vec![0, 1, 2, 3, 4]
    );
    assert_eq!(
        q(f64::NEG_INFINITY, f64::INFINITY, Some(1)).await,
        vec![0, 1, 3]
    );
    assert_eq!(
        q(f64::NEG_INFINITY, f64::INFINITY, Some(3)).await,
        Vec::<u32>::new()
    );
    // An inverted window is empty, not an error.
    assert_eq!(q(30.0, 10.0, None).await, Vec::<u32>::new());
}

#[tokio::test]
async fn run_id_filters_escape_quotes() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();

    let tricky = "o'brien' OR run_id = 'victim";
    store
        .append_spectra(&[
            synth_spectrum(tricky, 1, 1, Some(1.0)),
            synth_spectrum("victim", 2, 1, Some(2.0)),
            synth_spectrum("o'brien", 3, 1, Some(3.0)),
        ])
        .await
        .unwrap();
    store
        .append_run(&Run {
            run_id: tricky.into(),
            source_format: "mzml".into(),
            ..Default::default()
        })
        .await
        .unwrap();

    let got = batches_to_spectra(&store.read_spectra(tricky).await.unwrap());
    assert_eq!(scan_nums(&got), vec![1]);
    assert!(got.iter().all(|s| s.run_id == tricky));

    let got = batches_to_spectra(
        &store
            .query_window(tricky, f64::NEG_INFINITY, f64::INFINITY, None)
            .await
            .unwrap(),
    );
    assert_eq!(scan_nums(&got), vec![1]);

    let got = batches_to_spectra(&store.read_spectra("o'brien").await.unwrap());
    assert_eq!(scan_nums(&got), vec![3]);

    let run = store.read_run(tricky).await.unwrap().expect("run present");
    assert_eq!(batches_to_runs(&[run])[0].run_id, tricky);
}

#[tokio::test]
async fn indexed_queries_return_the_same_rows_as_scans() {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open(tmp.path().to_str().unwrap()).await.unwrap();

    for chunk in 0..4u32 {
        let mut batch = Vec::new();
        for i in 0..100u32 {
            let scan = chunk * 100 + i;
            let rt = if scan % 37 == 0 {
                None
            } else {
                Some(scan as f64 * 0.5)
            };
            batch.push(synth_spectrum("idx", scan, 1 + (scan % 3) as u8, rt));
            batch.push(synth_spectrum("other", scan, 1, rt));
        }
        store.append_spectra(&batch).await.unwrap();
    }
    store
        .append_run(&Run {
            run_id: "idx".into(),
            source_format: "mzml".into(),
            ..Default::default()
        })
        .await
        .unwrap();

    let windows: [(f64, f64, Option<u8>); 6] = [
        (f64::NEG_INFINITY, f64::INFINITY, None),
        (f64::NEG_INFINITY, f64::INFINITY, Some(2)),
        (10.0, 50.0, None),
        (10.0, 50.0, Some(1)),
        (0.0, 0.0, None),
        (150.0, f64::INFINITY, Some(3)),
    ];

    async fn run_all(
        store: &Store,
        windows: &[(f64, f64, Option<u8>)],
    ) -> Vec<Vec<serde_json::Value>> {
        let mut out = Vec::new();
        for &(lo, hi, lvl) in windows {
            let mut specs =
                batches_to_spectra(&store.query_window("idx", lo, hi, lvl).await.unwrap());
            specs.sort_by_key(|s| s.scan_num);
            out.push(as_json(&specs));
        }
        let mut all = batches_to_spectra(&store.read_spectra("idx").await.unwrap());
        all.sort_by_key(|s| s.scan_num);
        out.push(as_json(&all));
        out
    }

    let before = run_all(&store, &windows).await;
    // Sanity: the windows are not trivially empty.
    assert_eq!(before[0].len(), 400);
    assert!(before[3].len() > 1);

    store.create_default_indexes().await.unwrap();
    let spectra = store.spectra_table().await.unwrap();
    let mut indexed: Vec<Vec<String>> = spectra
        .list_indices()
        .await
        .unwrap()
        .into_iter()
        .map(|i| i.columns)
        .collect();
    indexed.sort();
    assert_eq!(
        indexed,
        vec![
            vec!["ms_level".to_string()],
            vec!["rt".to_string()],
            vec!["run_id".to_string()],
            vec!["scan_num".to_string()],
        ]
    );

    let after = run_all(&store, &windows).await;
    assert_eq!(before, after, "index path changed query results");

    // Rows appended after indexing (not yet covered by the index) are still
    // returned.
    store
        .append_spectra(&[synth_spectrum("idx", 1000, 1, Some(25.0))])
        .await
        .unwrap();
    let late = batches_to_spectra(&store.query_window("idx", 25.0, 25.0, None).await.unwrap());
    let mut late_scans = scan_nums(&late);
    late_scans.sort_unstable();
    assert_eq!(late_scans, vec![50, 1000]);
}
