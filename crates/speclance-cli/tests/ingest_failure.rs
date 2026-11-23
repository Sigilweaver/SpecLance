//! Ingest through the `speclance` binary: a failed or interrupted ingest
//! leaves nothing behind that blocks a retry of the same file, and a run
//! that is already in the store is refused.

use std::path::Path;
use std::process::{Command, Output};

use speclance_core::Store;

/// Spectra per write batch in the streaming ingest (`SPECTRUM_CHUNK`).
/// The failing file has more spectra than this, so a first batch is
/// written before the bad `index` is reached.
const SPECTRA: usize = 2050;

const HEAD: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<mzML xmlns="http://psi.hupo.org/ms/mzml" version="1.1.0" id="tiny">
  <cvList count="2">
    <cv id="MS" fullName="Mass spectrometry ontology" version="4.1" URI="http://psi-ms.obo"/>
    <cv id="UO" fullName="Unit Ontology" version="1.0" URI="http://uo.obo"/>
  </cvList>
  <fileDescription><fileContent><cvParam cvRef="MS" accession="MS:1000579" name="MS1 spectrum"/></fileContent></fileDescription>
  <softwareList count="1"><software id="t" version="0.1"><cvParam cvRef="MS" accession="MS:1000799" name="custom unreleased software tool" value="t"/></software></softwareList>
  <instrumentConfigurationList count="1"><instrumentConfiguration id="IC1"><componentList count="3"><source order="1"/><analyzer order="2"/><detector order="3"/></componentList></instrumentConfiguration></instrumentConfigurationList>
  <dataProcessingList count="1"><dataProcessing id="dp"><processingMethod order="0" softwareRef="t"><cvParam cvRef="MS" accession="MS:1000544" name="Conversion to mzML"/></processingMethod></dataProcessing></dataProcessingList>
  <run id="tiny" defaultInstrumentConfigurationRef="IC1" startTimeStamp="2024-01-01T00:00:00Z">
    <spectrumList count="@COUNT@" defaultDataProcessingRef="dp">
"#;
const SPECTRUM: &str = r#"      <spectrum index="@INDEX@" id="scan=@SCAN@" defaultArrayLength="3">
        <cvParam cvRef="MS" accession="MS:1000579" name="MS1 spectrum"/>
        <cvParam cvRef="MS" accession="MS:1000511" name="ms level" value="1"/>
        <cvParam cvRef="MS" accession="MS:1000130" name="positive scan"/>
        <cvParam cvRef="MS" accession="MS:1000127" name="centroid spectrum"/>
        <cvParam cvRef="MS" accession="MS:1000285" name="total ion current" value="300.5"/>
        <scanList count="1">
          <cvParam cvRef="MS" accession="MS:1000795" name="no combination"/>
          <scan>
            <cvParam cvRef="MS" accession="MS:1000016" name="scan start time" value="1.5" unitCvRef="UO" unitAccession="UO:0000010" unitName="second"/>
          </scan>
        </scanList>
        <binaryDataArrayList count="2">
          <binaryDataArray encodedLength="32">
            <cvParam cvRef="MS" accession="MS:1000523" name="64-bit float"/>
            <cvParam cvRef="MS" accession="MS:1000576" name="no compression"/>
            <cvParam cvRef="MS" accession="MS:1000514" name="m/z array"/>
            <binary>AAAAAAAAREAAAAAAAABJQAAAAAAAAFBA</binary>
          </binaryDataArray>
          <binaryDataArray encodedLength="16">
            <cvParam cvRef="MS" accession="MS:1000521" name="32-bit float"/>
            <cvParam cvRef="MS" accession="MS:1000576" name="no compression"/>
            <cvParam cvRef="MS" accession="MS:1000515" name="intensity array"/>
            <binary>AABAQAAAoEAAAPBA</binary>
          </binaryDataArray>
        </binaryDataArrayList>
      </spectrum>
"#;
const TAIL: &str = r#"    </spectrumList>
  </run>
</mzML>
"#;

fn mzml(spectra: usize, last_index: &str) -> String {
    let mut out = HEAD.replace("@COUNT@", &spectra.to_string());
    for i in 0..spectra {
        let index = if i + 1 == spectra {
            last_index.to_string()
        } else {
            i.to_string()
        };
        out.push_str(
            &SPECTRUM
                .replace("@INDEX@", &index)
                .replace("@SCAN@", &(i + 1).to_string()),
        );
    }
    out.push_str(TAIL);
    out
}

fn ingest(store: &Path, file: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_speclance"))
        .arg("ingest")
        .arg("--store")
        .arg(store)
        .arg(file)
        .output()
        .expect("run speclance")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

async fn rows(store: &Store, table: &str) -> usize {
    let names = store.table_names().await.unwrap();
    if !names.iter().any(|n| n == table) {
        return 0;
    }
    let t = store.conn.open_table(table).execute().await.unwrap();
    t.count_rows(None).await.unwrap()
}

#[tokio::test]
async fn failed_streaming_ingest_leaves_no_rows_and_the_file_can_be_retried() {
    let tmp = tempfile::tempdir().unwrap();
    let store_dir = tmp.path().join("store");
    let file = tmp.path().join("run.mzML");

    // The last spectrum has a non-numeric index. The first 2048 spectra
    // are already written when the reader reaches it.
    std::fs::write(&file, mzml(SPECTRA, "x")).unwrap();
    let failed = ingest(&store_dir, &file);
    assert!(!failed.status.success(), "{}", stderr(&failed));
    let msg = stderr(&failed);
    assert!(msg.contains("spectrum index"), "{msg}");
    assert!(
        msg.contains("removed 2048 rows"),
        "cleanup is reported: {msg}"
    );

    let store = Store::open(store_dir.to_str().unwrap()).await.unwrap();
    assert_eq!(rows(&store, "spectra").await, 0);
    assert_eq!(rows(&store, "runs").await, 0);

    // Same path and same byte length, so the same run_id.
    std::fs::write(&file, mzml(SPECTRA, "9")).unwrap();
    let retried = ingest(&store_dir, &file);
    assert!(retried.status.success(), "{}", stderr(&retried));
    assert_eq!(rows(&store, "spectra").await, SPECTRA);
    assert_eq!(rows(&store, "runs").await, 1);

    // The run is now committed: a second ingest is refused and adds nothing.
    let again = ingest(&store_dir, &file);
    assert!(!again.status.success());
    assert!(
        stderr(&again).contains("already in this store"),
        "{}",
        stderr(&again)
    );
    assert_eq!(rows(&store, "spectra").await, SPECTRA);
    assert_eq!(rows(&store, "runs").await, 1);
}

#[tokio::test]
async fn leftover_rows_of_an_interrupted_ingest_are_removed_before_a_retry() {
    let tmp = tempfile::tempdir().unwrap();
    let store_dir = tmp.path().join("store");
    let file = tmp.path().join("run.mzML");
    std::fs::write(&file, mzml(3, "2")).unwrap();

    // Spectra of this run without a run row, as an interrupted ingest leaves.
    let run_id = speclance_ms::mzml::read_mzml(&file).unwrap().run.run_id;

    let store = Store::open(store_dir.to_str().unwrap()).await.unwrap();
    let orphan = speclance_core::Spectrum {
        run_id: run_id.clone(),
        scan_num: 99,
        ms_level: 1,
        mz: vec![1.0],
        intensity: vec![1.0],
        ..Default::default()
    };
    store
        .append_spectra(&[orphan.clone(), orphan])
        .await
        .unwrap();
    assert!(!store.contains_run(&run_id).await.unwrap());

    let out = ingest(&store_dir, &file);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("removed 2 leftover rows"),
        "{}",
        stderr(&out)
    );
    assert_eq!(rows(&store, "spectra").await, 3);
    assert_eq!(rows(&store, "runs").await, 1);
}
