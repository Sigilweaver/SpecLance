//! Roundtrip tests for the mzML reader and writer.

use prolance_ms::mzml::{parse_bytes, read_mzml, write_mzml};

const TINY_MZML: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<mzML xmlns="http://psi.hupo.org/ms/mzml" version="1.1.0" id="tiny">
  <cvList count="2">
    <cv id="MS" fullName="Mass spectrometry ontology" version="4.1" URI="http://psi-ms.obo"/>
    <cv id="UO" fullName="Unit Ontology" version="1.0" URI="http://uo.obo"/>
  </cvList>
  <fileDescription><fileContent><cvParam cvRef="MS" accession="MS:1000579" name="MS1 spectrum"/></fileContent></fileDescription>
  <softwareList count="1"><software id="test" version="0.1"><cvParam cvRef="MS" accession="MS:1000799" name="custom unreleased software tool" value="test"/></software></softwareList>
  <instrumentConfigurationList count="1"><instrumentConfiguration id="IC1"><componentList count="3"><source order="1"/><analyzer order="2"/><detector order="3"/></componentList></instrumentConfiguration></instrumentConfigurationList>
  <dataProcessingList count="1"><dataProcessing id="dp"><processingMethod order="0" softwareRef="test"><cvParam cvRef="MS" accession="MS:1000544" name="Conversion to mzML"/></processingMethod></dataProcessing></dataProcessingList>
  <run id="tiny" defaultInstrumentConfigurationRef="IC1" startTimeStamp="2024-01-01T00:00:00Z">
    <spectrumList count="1" defaultDataProcessingRef="dp">
      <spectrum index="0" id="scan=1" defaultArrayLength="3">
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
    </spectrumList>
  </run>
</mzML>
"#;

#[test]
fn read_synthetic_mzml() {
    let data = parse_bytes(TINY_MZML.as_bytes(), "tiny.mzML".into()).unwrap();
    assert_eq!(data.spectra.len(), 1);
    let s = &data.spectra[0];
    assert_eq!(s.ms_level, 1);
    assert_eq!(s.polarity, Some(1));
    assert_eq!(s.centroided, Some(true));
    assert_eq!(s.tic, Some(300.5));
    assert_eq!(s.rt, Some(1.5));
    assert_eq!(s.mz.len(), 3);
    assert_eq!(s.intensity.len(), 3);
    assert!((s.mz[0] - 40.0).abs() < 1e-9);
    assert!((s.mz[1] - 50.0).abs() < 1e-9);
    assert!((s.mz[2] - 64.0).abs() < 1e-9);
    assert!((s.intensity[0] - 3.0).abs() < 1e-6);
    assert!((s.intensity[1] - 5.0).abs() < 1e-6);
    assert!((s.intensity[2] - 7.5).abs() < 1e-6);
}

#[test]
fn roundtrip_synthetic_mzml() {
    let first = parse_bytes(TINY_MZML.as_bytes(), "tiny.mzML".into()).unwrap();

    let mut buf: Vec<u8> = Vec::new();
    write_mzml(&mut buf, &first.run, &first.spectra, &first.chromatograms).unwrap();

    let second = parse_bytes(&buf, "tiny-rt.mzML".into()).unwrap();
    assert_eq!(second.spectra.len(), first.spectra.len());
    let a = &first.spectra[0];
    let b = &second.spectra[0];
    assert_eq!(a.ms_level, b.ms_level);
    assert_eq!(a.polarity, b.polarity);
    assert_eq!(a.centroided, b.centroided);
    assert_eq!(a.tic, b.tic);
    assert_eq!(a.rt, b.rt);
    assert_eq!(a.mz, b.mz);
    assert_eq!(a.intensity, b.intensity);
    assert_eq!(a.native_id, b.native_id);
}

#[test]
fn roundtrip_real_thermo_mzml_if_present() {
    let path = "/workspaces/ProLance/corpus/thermo/mzml/PXD069101_TSQ_Altis_milla0255_01.mzML";
    if std::fs::metadata(path).is_err() {
        eprintln!("skipping: corpus not present");
        return;
    }
    let first = read_mzml(path).expect("read source");
    let mut buf: Vec<u8> = Vec::new();
    write_mzml(&mut buf, &first.run, &first.spectra, &first.chromatograms).unwrap();
    let second = parse_bytes(&buf, format!("{}-rt", path)).expect("read roundtripped");
    assert_eq!(first.spectra.len(), second.spectra.len(), "spectrum count");
    assert_eq!(
        first.chromatograms.len(),
        second.chromatograms.len(),
        "chromatogram count"
    );
    // Spot-check the first and last spectra: peak counts and a few values.
    let pairs: &[(usize, usize)] = &[(0, 0), (first.spectra.len() - 1, second.spectra.len() - 1)];
    for &(i, j) in pairs {
        let a = &first.spectra[i];
        let b = &second.spectra[j];
        assert_eq!(a.mz.len(), b.mz.len(), "mz len for spectrum {}", i);
        assert_eq!(a.intensity.len(), b.intensity.len(), "int len {}", i);
        for k in 0..a.mz.len() {
            assert!((a.mz[k] - b.mz[k]).abs() < 1e-6, "mz[{}][{}]", i, k);
            assert!(
                (a.intensity[k] - b.intensity[k]).abs() < 1e-3,
                "int[{}][{}]",
                i,
                k
            );
        }
        assert_eq!(a.ms_level, b.ms_level);
        assert_eq!(a.rt.is_some(), b.rt.is_some());
        if let (Some(ra), Some(rb)) = (a.rt, b.rt) {
            assert!((ra - rb).abs() < 1e-6, "rt mismatch spectrum {}", i);
        }
    }
}
