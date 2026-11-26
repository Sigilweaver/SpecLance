//! What `speclance ingest` and `speclance export` report on stderr: skipped
//! binary data arrays on ingest, and what export wrote.

use std::path::Path;
use std::process::{Command, Output};

const MZ_B64: &str = "AAAAAAAAREAAAAAAAABJQAAAAAAAAFBA";
const INT_B64: &str = "AABAQAAAoEAAAPBA";

/// One spectrum with an m/z array, an intensity array and a mean ion
/// mobility array, and optionally a TIC chromatogram.
fn mzml(with_chromatogram: bool) -> String {
    let chrom = if with_chromatogram {
        format!(
            r#"    <chromatogramList count="1" defaultDataProcessingRef="dp">
      <chromatogram index="0" id="TIC" defaultArrayLength="3">
        <cvParam cvRef="MS" accession="MS:1000235" name="total ion current chromatogram"/>
        <binaryDataArrayList count="2">
          <binaryDataArray encodedLength="32"><cvParam cvRef="MS" accession="MS:1000523" name="64-bit float"/><cvParam cvRef="MS" accession="MS:1000576" name="no compression"/><cvParam cvRef="MS" accession="MS:1000595" name="time array"/><binary>{MZ_B64}</binary></binaryDataArray>
          <binaryDataArray encodedLength="16"><cvParam cvRef="MS" accession="MS:1000521" name="32-bit float"/><cvParam cvRef="MS" accession="MS:1000576" name="no compression"/><cvParam cvRef="MS" accession="MS:1000515" name="intensity array"/><binary>{INT_B64}</binary></binaryDataArray>
        </binaryDataArrayList>
      </chromatogram>
    </chromatogramList>
"#
        )
    } else {
        String::new()
    };
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
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
    <spectrumList count="1" defaultDataProcessingRef="dp">
      <spectrum index="0" id="scan=1" defaultArrayLength="3">
        <cvParam cvRef="MS" accession="MS:1000579" name="MS1 spectrum"/>
        <cvParam cvRef="MS" accession="MS:1000511" name="ms level" value="1"/>
        <cvParam cvRef="MS" accession="MS:1000130" name="positive scan"/>
        <cvParam cvRef="MS" accession="MS:1000127" name="centroid spectrum"/>
        <scanList count="1">
          <cvParam cvRef="MS" accession="MS:1000795" name="no combination"/>
          <scan>
            <cvParam cvRef="MS" accession="MS:1000016" name="scan start time" value="1.5" unitCvRef="UO" unitAccession="UO:0000010" unitName="second"/>
          </scan>
        </scanList>
        <binaryDataArrayList count="3">
          <binaryDataArray encodedLength="32">
            <cvParam cvRef="MS" accession="MS:1000523" name="64-bit float"/>
            <cvParam cvRef="MS" accession="MS:1000576" name="no compression"/>
            <cvParam cvRef="MS" accession="MS:1000514" name="m/z array"/>
            <binary>{MZ_B64}</binary>
          </binaryDataArray>
          <binaryDataArray encodedLength="16">
            <cvParam cvRef="MS" accession="MS:1000521" name="32-bit float"/>
            <cvParam cvRef="MS" accession="MS:1000576" name="no compression"/>
            <cvParam cvRef="MS" accession="MS:1000515" name="intensity array"/>
            <binary>{INT_B64}</binary>
          </binaryDataArray>
          <binaryDataArray encodedLength="16">
            <cvParam cvRef="MS" accession="MS:1000521" name="32-bit float"/>
            <cvParam cvRef="MS" accession="MS:1000576" name="no compression"/>
            <cvParam cvRef="MS" accession="MS:1002816" name="mean ion mobility array"/>
            <binary>{INT_B64}</binary>
          </binaryDataArray>
        </binaryDataArrayList>
      </spectrum>
    </spectrumList>
{chrom}  </run>
</mzML>
"#
    )
}

fn speclance(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_speclance"))
        .args(args)
        .output()
        .expect("run speclance")
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn ingest(store: &Path, file: &Path) -> Output {
    speclance(&[
        "ingest".as_ref(),
        "--store".as_ref(),
        store.as_os_str(),
        file.as_os_str(),
    ])
}

fn export(store: &Path, run_id: &str, out: &Path) -> Output {
    speclance(&[
        "export".as_ref(),
        "--store".as_ref(),
        store.as_os_str(),
        "--run-id".as_ref(),
        run_id.as_ref(),
        "--out".as_ref(),
        out.as_os_str(),
    ])
}

#[test]
fn ingest_reports_skipped_arrays() {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("run.mzML");
    std::fs::write(&file, mzml(false)).unwrap();
    let out = ingest(&tmp.path().join("store"), &file);
    assert!(out.status.success(), "{}", stderr(&out));
    let msg = stderr(&out);
    assert!(
        msg.contains(
            "skipped 1 binary data arrays that are not m/z or intensity (mean ion mobility array: 1)"
        ),
        "{msg}"
    );
}

#[test]
fn export_says_what_it_wrote_and_that_chromatograms_are_not_exported() {
    let tmp = tempfile::tempdir().unwrap();
    for (name, with_chromatogram) in [("plain", false), ("chrom", true)] {
        let dir = tmp.path().join(name);
        std::fs::create_dir(&dir).unwrap();
        let file = dir.join("run.mzML");
        std::fs::write(&file, mzml(with_chromatogram)).unwrap();
        let store = dir.join("store");
        let out = ingest(&store, &file);
        assert!(out.status.success(), "{}", stderr(&out));

        let run_id = speclance_ms::mzml::read_mzml(&file).unwrap().run.run_id;
        let exported = dir.join("out.mzML");
        let out = export(&store, &run_id, &exported);
        assert!(out.status.success(), "{}", stderr(&out));
        let msg = stderr(&out);
        assert!(msg.contains("wrote 1 spectra ->"), "{name}: {msg}");
        assert!(!msg.contains("and 0 chromatograms"), "{name}: {msg}");
        if with_chromatogram {
            assert!(
                msg.contains("1 chromatograms, which are not exported"),
                "{msg}"
            );
        } else {
            assert!(!msg.contains("chromatogram"), "{msg}");
        }
    }
}
