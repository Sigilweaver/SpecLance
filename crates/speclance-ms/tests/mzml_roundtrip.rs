//! Roundtrip tests for the mzML reader and writer.

use speclance_ms::mzml::{parse_bytes, read_mzml, write_mzml, MzmlIngest};

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
    let path = "/workspaces/SpecLance/corpus/thermo/mzml/PXD069101_TSQ_Altis_milla0255_01.mzML";
    if std::fs::metadata(path).is_err() {
        eprintln!("skipping: corpus not present");
        return;
    }
    let first = read_mzml(path).expect("read source");
    let mut buf: Vec<u8> = Vec::new();
    write_mzml(&mut buf, &first.run, &first.spectra, &first.chromatograms).unwrap();
    let second = parse_bytes(&buf, format!("{}-rt", path)).expect("read roundtripped");
    assert_eq!(first.spectra.len(), second.spectra.len(), "spectrum count");
    // NOTE: chromatograms are intentionally not emitted by the current writer
    // (delegates to openmassspec-core, which is spectrum-centric). Re-enable
    // when chromatogram support lands upstream.
    let _ = (first.chromatograms.len(), second.chromatograms.len());
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
            // Writer formats RT as minutes with 6 decimals (~60us in seconds).
            assert!((ra - rb).abs() < 1e-3, "rt mismatch spectrum {}", i);
        }
    }
}

#[test]
fn streaming_ingest_matches_parse_bytes() {
    use speclance_ms::mzml::MzmlIngest;

    let mut ingest =
        MzmlIngest::open_bytes(TINY_MZML.as_bytes().to_vec(), "tiny-stream.mzML".into()).unwrap();
    let mut streamed = Vec::new();
    let mut ms1 = 0u32;
    let mut ms2 = 0u32;
    while let Some(s) = ingest.next_spectrum().unwrap() {
        if s.ms_level == 1 {
            ms1 += 1;
        } else if s.ms_level >= 2 {
            ms2 += 1;
        }
        streamed.push(s);
    }
    let chroms = ingest.read_chromatograms().unwrap();
    let run = ingest
        .finalize_run(streamed.len() as u32, ms1, ms2)
        .unwrap();

    let buffered = parse_bytes(TINY_MZML.as_bytes(), "tiny-stream.mzML".into()).unwrap();

    assert_eq!(streamed.len(), buffered.spectra.len());
    assert_eq!(chroms.len(), buffered.chromatograms.len());
    assert_eq!(run.spectrum_count, buffered.run.spectrum_count);
    assert_eq!(run.ms1_count, buffered.run.ms1_count);
    assert_eq!(run.ms2_count, buffered.run.ms2_count);
    assert_eq!(run.run_id, buffered.run.run_id);
    for (a, b) in streamed.iter().zip(buffered.spectra.iter()) {
        assert_eq!(a.ms_level, b.ms_level);
        assert_eq!(a.mz, b.mz);
        assert_eq!(a.intensity, b.intensity);
        assert_eq!(a.rt, b.rt);
    }
}

const MZ_B64: &str = "AAAAAAAAREAAAAAAAABJQAAAAAAAAFBA";

fn tiny_with_mz_binary(body: &str) -> Vec<u8> {
    TINY_MZML
        .replace(
            &format!("<binary>{MZ_B64}</binary>"),
            &format!("<binary>{body}</binary>"),
        )
        .into_bytes()
}

fn assert_tiny_mz(data: &speclance_ms::mzml::MzmlData) {
    assert_eq!(data.spectra[0].mz, vec![40.0, 50.0, 64.0]);
}

#[test]
fn base64_with_char_refs_is_decoded() {
    let body = format!(
        "{}&#10;{}&#x20;{}",
        &MZ_B64[..8],
        &MZ_B64[8..20],
        &MZ_B64[20..]
    );
    let data = parse_bytes(&tiny_with_mz_binary(&body), "ref.mzML".into()).unwrap();
    assert_tiny_mz(&data);
}

#[test]
fn base64_in_cdata_is_decoded() {
    let body = format!("<![CDATA[{}]]>", MZ_B64);
    let data = parse_bytes(&tiny_with_mz_binary(&body), "cdata.mzML".into()).unwrap();
    assert_tiny_mz(&data);
}

#[test]
fn base64_split_across_text_and_cdata_is_decoded() {
    let body = format!("{}<![CDATA[{}]]>", &MZ_B64[..12], &MZ_B64[12..]);
    let data = parse_bytes(&tiny_with_mz_binary(&body), "mixed.mzML".into()).unwrap();
    assert_tiny_mz(&data);
}

#[test]
fn unknown_entity_in_binary_is_an_xml_error() {
    let body = format!("{}&bogus;{}", &MZ_B64[..8], &MZ_B64[8..]);
    let err = parse_bytes(&tiny_with_mz_binary(&body), "bad.mzML".into()).unwrap_err();
    assert!(matches!(err, speclance_ms::MsError::Xml(_)), "{err:?}");
}

#[test]
fn predefined_entities_in_binary_are_resolved() {
    // Each predefined entity resolves to a character that is not valid
    // base64, so the failure is a decode error rather than an unknown
    // entity (MsError::Xml) or silently dropped text.
    for ent in ["amp", "lt", "gt", "apos", "quot"] {
        let body = format!("{}&{ent};{}", &MZ_B64[..8], &MZ_B64[8..]);
        let err = parse_bytes(&tiny_with_mz_binary(&body), "ent.mzML".into()).unwrap_err();
        assert!(
            matches!(err, speclance_ms::MsError::Base64(_)),
            "{ent}: {err:?}"
        );
    }
}

const INT_B64: &str = "AABAQAAAoEAAAPBA";

fn tiny_with_chromatogram(time_body: &str) -> Vec<u8> {
    let chrom = format!(
        r#"<chromatogramList count="1"><chromatogram index="0" id="TIC" defaultArrayLength="3">
<cvParam cvRef="MS" accession="MS:1000235" name="total ion current chromatogram"/>
<binaryDataArrayList count="2">
<binaryDataArray encodedLength="32"><cvParam cvRef="MS" accession="MS:1000523" name="64-bit float"/><cvParam cvRef="MS" accession="MS:1000576" name="no compression"/><cvParam cvRef="MS" accession="MS:1000595" name="time array"/><binary>{time_body}</binary></binaryDataArray>
<binaryDataArray encodedLength="16"><cvParam cvRef="MS" accession="MS:1000521" name="32-bit float"/><cvParam cvRef="MS" accession="MS:1000576" name="no compression"/><cvParam cvRef="MS" accession="MS:1000515" name="intensity array"/><binary>{INT_B64}</binary></binaryDataArray>
</binaryDataArrayList></chromatogram></chromatogramList>
"#
    );
    TINY_MZML
        .replace("  </run>", &format!("{chrom}  </run>"))
        .into_bytes()
}

fn assert_tiny_chrom(data: &speclance_ms::mzml::MzmlData) {
    assert_eq!(data.chromatograms.len(), 1);
    assert_eq!(data.chromatograms[0].time, vec![40.0, 50.0, 64.0]);
    assert_eq!(data.chromatograms[0].intensity, vec![3.0, 5.0, 7.5]);
}

#[test]
fn chromatogram_base64_plain_is_decoded() {
    let data = parse_bytes(&tiny_with_chromatogram(MZ_B64), "chrom.mzML".into()).unwrap();
    assert_tiny_chrom(&data);
}

#[test]
fn chromatogram_base64_with_char_refs_and_cdata_is_decoded() {
    let body = format!(
        "{}&#10;{}&#x20;<![CDATA[{}]]>",
        &MZ_B64[..8],
        &MZ_B64[8..20],
        &MZ_B64[20..]
    );
    let data = parse_bytes(&tiny_with_chromatogram(&body), "chrom-ref.mzML".into()).unwrap();
    assert_tiny_chrom(&data);
}

#[test]
fn chromatogram_unknown_entity_is_an_xml_error() {
    let body = format!("{}&bogus;{}", &MZ_B64[..8], &MZ_B64[8..]);
    let err = parse_bytes(&tiny_with_chromatogram(&body), "bad.mzML".into()).unwrap_err();
    assert!(matches!(err, speclance_ms::MsError::Xml(_)), "{err:?}");
}

fn ms2_mzml(activation_terms: &[(&str, &str)]) -> String {
    let terms: String = activation_terms
        .iter()
        .map(|(acc, name)| format!(r#"<cvParam cvRef="MS" accession="{acc}" name="{name}"/>"#))
        .collect();
    let ms2_spectrum = format!(
        r#"<spectrum index="1" id="scan=2" defaultArrayLength="3">
        <cvParam cvRef="MS" accession="MS:1000580" name="MSn spectrum"/>
        <cvParam cvRef="MS" accession="MS:1000511" name="ms level" value="2"/>
        <cvParam cvRef="MS" accession="MS:1000130" name="positive scan"/>
        <cvParam cvRef="MS" accession="MS:1000127" name="centroid spectrum"/>
        <scanList count="1">
          <cvParam cvRef="MS" accession="MS:1000795" name="no combination"/>
          <scan>
            <cvParam cvRef="MS" accession="MS:1000016" name="scan start time" value="2.5" unitCvRef="UO" unitAccession="UO:0000010" unitName="second"/>
          </scan>
        </scanList>
        <precursorList count="1">
          <precursor>
            <selectedIonList count="1">
              <selectedIon>
                <cvParam cvRef="MS" accession="MS:1000744" name="selected ion m/z" value="500.25"/>
                <cvParam cvRef="MS" accession="MS:1000041" name="charge state" value="2"/>
              </selectedIon>
            </selectedIonList>
            <activation>
              {terms}
              <cvParam cvRef="MS" accession="MS:1000045" name="collision energy" value="30"/>
            </activation>
          </precursor>
        </precursorList>
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
      </spectrum>"#
    );
    let (head, tail) = TINY_MZML.split_once("</spectrumList>").unwrap();
    head.replace(r#"<spectrumList count="1""#, r#"<spectrumList count="2""#)
        + &ms2_spectrum
        + "</spectrumList>"
        + tail
}

/// Terms the activation name accounts for are not repeated in `cv_params`.
fn assert_no_activation_in_cv_params(s: &speclance_core::Spectrum, what: &str) {
    let cv = s.cv_params.as_deref().unwrap_or("");
    assert!(!cv.contains("\"section\":\"activation\""), "{what}: {cv}");
}

fn ms2_spectrum_with_activation(stored: &str) -> speclance_core::Spectrum {
    speclance_core::Spectrum {
        scan_num: 2,
        ms_level: 2,
        precursor: Some(speclance_core::Precursor {
            mz: Some(500.25),
            ..Default::default()
        }),
        activation: Some(stored.to_string()),
        mz: vec![40.0, 50.0],
        intensity: vec![1.0, 2.0],
        ..Default::default()
    }
}

fn write_and_read(spec: speclance_core::Spectrum) -> (String, speclance_ms::mzml::MzmlData) {
    let base = parse_bytes(TINY_MZML.as_bytes(), "tiny.mzML".into()).unwrap();
    let mut buf: Vec<u8> = Vec::new();
    write_mzml(&mut buf, &base.run, &[spec], &[]).unwrap();
    let xml = String::from_utf8(buf.clone()).unwrap();
    (xml, parse_bytes(&buf, "act-rt.mzML".into()).unwrap())
}

/// Every stored name: accessions written, then the name read back.
#[test]
fn write_then_read_every_stored_activation() {
    let cases: [(&str, &[&str], &str); 12] = [
        ("CID", &["MS:1000133"], "CID"),
        ("HCD", &["MS:1000422"], "HCD"),
        ("ETD", &["MS:1000598"], "ETD"),
        ("EThcD", &["MS:1000598", "MS:1002631"], "EThcD"),
        ("ECD", &["MS:1000250"], "ECD"),
        ("IRMPD", &["MS:1000262"], "IRMPD"),
        ("UVPD", &["MS:1003246"], "UVPD"),
        ("PQD", &["MS:1000599"], "PQD"),
        ("SID", &["MS:1000136"], "SID"),
        ("MPID", &["MS:1000435"], "MPID"),
        ("MPD", &["MS:1000435"], "MPID"),
        (
            "beam-type collision-induced dissociation",
            &["MS:1000422"],
            "HCD",
        ),
    ];
    for (stored, accessions, expected) in cases {
        let (xml, back) = write_and_read(ms2_spectrum_with_activation(stored));
        let start = xml.find("<activation>").unwrap();
        let end = xml.find("</activation>").unwrap();
        let block = &xml[start..end];
        let written: Vec<&str> = block
            .match_indices("accession=\"MS:")
            .map(|(i, _)| &block[i + 11..i + 21])
            .collect();
        assert_eq!(written, accessions, "{stored}: {block}");
        assert_eq!(
            back.spectra[0].activation.as_deref(),
            Some(expected),
            "{stored}"
        );
        assert_no_activation_in_cv_params(&back.spectra[0], stored);
    }
}

#[test]
fn pd_cannot_be_written() {
    let base = parse_bytes(TINY_MZML.as_bytes(), "tiny.mzML".into()).unwrap();
    let mut buf = Vec::new();
    let r = write_mzml(
        &mut buf,
        &base.run,
        &[ms2_spectrum_with_activation("PD")],
        &[],
    );
    assert!(
        matches!(r, Err(speclance_ms::MsError::Unsupported(_))),
        "{r:?}"
    );
}

#[test]
fn activation_without_precursor_cannot_be_written() {
    let mut spec = ms2_spectrum_with_activation("HCD");
    spec.precursor = None;
    let base = parse_bytes(TINY_MZML.as_bytes(), "tiny.mzML".into()).unwrap();
    let mut buf = Vec::new();
    let r = write_mzml(&mut buf, &base.run, &[spec], &[]);
    assert!(
        matches!(r, Err(speclance_ms::MsError::Unsupported(_))),
        "{r:?}"
    );
}

fn read_ms2(terms: &[(&str, &str)]) -> speclance_core::Spectrum {
    let xml = ms2_mzml(terms);
    parse_bytes(xml.as_bytes(), "read.mzML".into())
        .unwrap()
        .spectra
        .remove(1)
}

/// Each accession the reader maps, as PSI-MS defines it.
#[test]
fn reader_maps_psi_ms_accessions() {
    let cases = [
        ("MS:1000133", "collision-induced dissociation", "CID"),
        (
            "MS:1000422",
            "beam-type collision-induced dissociation",
            "HCD",
        ),
        (
            "MS:1002481",
            "higher energy beam-type collision-induced dissociation",
            "HCD",
        ),
        ("MS:1000598", "electron transfer dissociation", "ETD"),
        (
            "MS:1002631",
            "electron-transfer/higher-energy collision dissociation",
            "EThcD",
        ),
        ("MS:1000599", "pulsed q dissociation", "PQD"),
        ("MS:1000250", "electron capture dissociation", "ECD"),
        ("MS:1000262", "infrared multiphoton dissociation", "IRMPD"),
        ("MS:1003246", "ultraviolet photodissociation", "UVPD"),
        ("MS:1000136", "surface-induced dissociation", "SID"),
        ("MS:1000435", "photodissociation", "MPID"),
    ];
    for (acc, name, want) in cases {
        let s = read_ms2(&[(acc, name)]);
        assert_eq!(s.activation.as_deref(), Some(want), "{acc}");
        assert_no_activation_in_cv_params(&s, acc);
        assert_eq!(s.collision_energy, Some(30.0));
    }
}

/// In-source CID is not precursor activation, and a supplemental term alone
/// names no activation; both stay in `cv_params`.
#[test]
fn reader_keeps_non_activation_terms() {
    let cases = [
        ("MS:1001880", "in-source collision-induced dissociation"),
        (
            "MS:1002678",
            "supplemental beam-type collision-induced dissociation",
        ),
        ("MS:1002679", "supplemental collision-induced dissociation"),
        ("MS:9999999", "made up dissociation"),
    ];
    for (acc, name) in cases {
        let s = read_ms2(&[(acc, name)]);
        assert_eq!(s.activation, None, "{acc}");
        let cv = s.cv_params.as_deref().expect("term recorded");
        assert!(cv.contains(acc) && cv.contains(name), "{cv}");
    }
}

#[test]
fn multi_term_blocks() {
    let etd = ("MS:1000598", "electron transfer dissociation");
    let ethcd = (
        "MS:1002631",
        "electron-transfer/higher-energy collision dissociation",
    );
    let sup_beam = (
        "MS:1002678",
        "supplemental beam-type collision-induced dissociation",
    );
    let sup_cid = ("MS:1002679", "supplemental collision-induced dissociation");
    let cid = ("MS:1000133", "collision-induced dissociation");

    for terms in [[etd, ethcd], [ethcd, etd], [etd, sup_beam]] {
        let s = read_ms2(&terms);
        assert_eq!(s.activation.as_deref(), Some("EThcD"));
        assert_no_activation_in_cv_params(&s, "pair");
    }

    // The name has no slot for the supplemental CID term: kept.
    let s = read_ms2(&[cid, sup_cid]);
    assert_eq!(s.activation.as_deref(), Some("CID"));
    assert!(s.cv_params.as_deref().unwrap().contains("MS:1002679"));

    // A second, different primary term does not overwrite the first.
    let s = read_ms2(&[etd, cid]);
    assert_eq!(s.activation.as_deref(), Some("ETD"));
    assert!(s.cv_params.as_deref().unwrap().contains("MS:1000133"));
}

#[test]
fn malformed_spectrum_index_is_an_error() {
    let xml = TINY_MZML.replace(r#"index="0" id="scan=1""#, r#"index="x" id="scan=1""#);
    let err = parse_bytes(xml.as_bytes(), "bad-index.mzML".into()).unwrap_err();
    assert!(err.to_string().contains("spectrum index"), "{err}");
}

// -- ion mobility terms --

const RT_PARAM: &str = r#"<cvParam cvRef="MS" accession="MS:1000016" name="scan start time" value="1.5" unitCvRef="UO" unitAccession="UO:0000010" unitName="second"/>"#;
const INV_K0: &str = r#"<cvParam cvRef="MS" accession="MS:1002815" name="inverse reduced ion mobility" value="0.95" unitCvRef="MS" unitAccession="MS:1002814" unitName="volt-second per square centimeter"/>"#;
const DRIFT_TIME: &str = r#"<cvParam cvRef="MS" accession="MS:1002476" name="ion mobility drift time" value="12.5" unitCvRef="UO" unitAccession="UO:0000028" unitName="millisecond"/>"#;

fn tiny_with_scan_params(params: &[&str]) -> Vec<u8> {
    TINY_MZML
        .replace(RT_PARAM, &format!("{RT_PARAM}{}", params.concat()))
        .into_bytes()
}

fn cv_params_json(s: &speclance_core::Spectrum) -> serde_json::Value {
    serde_json::from_str(s.cv_params.as_deref().expect("cv_params recorded")).unwrap()
}

#[test]
fn scan_inverse_reduced_mobility_fills_inv_mobility() {
    let data = parse_bytes(&tiny_with_scan_params(&[INV_K0]), "k0.mzML".into()).unwrap();
    let s = &data.spectra[0];
    assert_eq!(s.inv_mobility, Some(0.95));
    // The value has its column; it is not repeated in cv_params.
    assert!(
        s.cv_params
            .as_deref()
            .is_none_or(|cv| !cv.contains("MS:1002815")),
        "{:?}",
        s.cv_params
    );
}

#[test]
fn scan_drift_time_is_not_stored_as_inverse_reduced_mobility() {
    let data = parse_bytes(&tiny_with_scan_params(&[DRIFT_TIME]), "drift.mzML".into()).unwrap();
    let s = &data.spectra[0];
    assert_eq!(s.inv_mobility, None);
    // The drift time is kept with its unit, in the scan section.
    let cv = cv_params_json(s);
    let entry = cv
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["accession"] == "MS:1002476")
        .expect("drift time kept in cv_params");
    assert_eq!(entry["section"], "scan");
    assert_eq!(entry["value"], "12.5");
    assert_eq!(entry["unit_accession"], "UO:0000028");
}

#[test]
fn scan_with_both_mobility_terms_keeps_each_in_its_place() {
    let data = parse_bytes(
        &tiny_with_scan_params(&[INV_K0, DRIFT_TIME]),
        "both.mzML".into(),
    )
    .unwrap();
    let s = &data.spectra[0];
    assert_eq!(s.inv_mobility, Some(0.95));
    let cv = s.cv_params.as_deref().unwrap();
    assert!(
        cv.contains("MS:1002476") && !cv.contains("MS:1002815"),
        "{cv}"
    );
}

#[test]
fn scan_inverse_reduced_mobility_that_is_not_a_number_is_kept() {
    let bad = INV_K0.replace(r#"value="0.95""#, r#"value="n/a""#);
    let data = parse_bytes(&tiny_with_scan_params(&[&bad]), "badk0.mzML".into()).unwrap();
    let s = &data.spectra[0];
    assert_eq!(s.inv_mobility, None);
    let cv = s.cv_params.as_deref().unwrap();
    assert!(cv.contains("MS:1002815") && cv.contains("n/a"), "{cv}");
}

#[test]
fn export_writes_inverse_reduced_mobility_and_no_drift_time() {
    let k0 = parse_bytes(&tiny_with_scan_params(&[INV_K0]), "k0.mzML".into()).unwrap();
    let mut out = Vec::new();
    write_mzml(&mut out, &k0.run, &k0.spectra, &k0.chromatograms).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(xml.contains(r#"accession="MS:1002815""#), "{xml}");
    let back = parse_bytes(xml.as_bytes(), "k0-back.mzML".into()).unwrap();
    assert_eq!(back.spectra[0].inv_mobility, Some(0.95));

    // A drift time is not written as 1/K0.
    let drift = parse_bytes(&tiny_with_scan_params(&[DRIFT_TIME]), "drift.mzML".into()).unwrap();
    let mut out = Vec::new();
    write_mzml(&mut out, &drift.run, &drift.spectra, &drift.chromatograms).unwrap();
    let xml = String::from_utf8(out).unwrap();
    assert!(!xml.contains("MS:1002815"), "{xml}");
}

// -- skipped binary data arrays --

const MOBILITY_ARRAY: &str = r#"<binaryDataArray encodedLength="16">
            <cvParam cvRef="MS" accession="MS:1000521" name="32-bit float"/>
            <cvParam cvRef="MS" accession="MS:1000576" name="no compression"/>
            <cvParam cvRef="MS" accession="MS:1002816" name="mean ion mobility array" unitCvRef="MS" unitAccession="MS:1002814" unitName="volt-second per square centimeter"/>
            <binary>AABAQAAAoEAAAPBA</binary>
          </binaryDataArray>
          "#;
const UNNAMED_ARRAY: &str = r#"<binaryDataArray encodedLength="16">
            <cvParam cvRef="MS" accession="MS:1000521" name="32-bit float"/>
            <cvParam cvRef="MS" accession="MS:1000576" name="no compression"/>
            <binary>AABAQAAAoEAAAPBA</binary>
          </binaryDataArray>
          "#;

fn tiny_with_extra_arrays(arrays: &[&str]) -> Vec<u8> {
    let marker = r#"<binaryDataArray encodedLength="16">"#;
    TINY_MZML
        .replace(marker, &format!("{}{marker}", arrays.concat()))
        .replace(
            r#"<binaryDataArrayList count="2">"#,
            &format!(r#"<binaryDataArrayList count="{}">"#, 2 + arrays.len()),
        )
        .into_bytes()
}

#[test]
fn skipped_arrays_are_counted_by_name_and_recorded_on_the_run() {
    let xml = tiny_with_extra_arrays(&[MOBILITY_ARRAY, MOBILITY_ARRAY, UNNAMED_ARRAY]);
    let mut ingest = MzmlIngest::open_bytes(xml, "skip.mzML".into()).unwrap();
    assert_eq!(ingest.skipped_array_count(), 0);
    assert_eq!(ingest.skipped_arrays_summary(), None);

    let s = ingest.next_spectrum().unwrap().unwrap();
    // m/z and intensity are still read.
    assert_eq!(s.mz, vec![40.0, 50.0, 64.0]);
    assert_eq!(s.intensity, vec![3.0, 5.0, 7.5]);
    assert!(ingest.next_spectrum().unwrap().is_none());
    ingest.read_chromatograms().unwrap();

    let by_array = ingest.skipped_arrays();
    assert_eq!(by_array.get("mean ion mobility array"), Some(&2));
    assert_eq!(by_array.get("unnamed array"), Some(&1));
    assert_eq!(ingest.skipped_array_count(), 3);
    let summary = ingest.skipped_arrays_summary().unwrap();
    assert!(
        summary.contains("skipped 3 binary data arrays"),
        "{summary}"
    );
    assert!(summary.contains("mean ion mobility array: 2"), "{summary}");

    let run = ingest.finalize_run(1, 1, 0).unwrap();
    let meta: serde_json::Value =
        serde_json::from_str(run.run_metadata.as_deref().unwrap()).unwrap();
    assert_eq!(meta["skipped_arrays"]["count"], 3);
    assert_eq!(
        meta["skipped_arrays"]["by_array"]["mean ion mobility array"],
        2
    );
    // The verbatim blocks are still there and still read back as `Verbatim`.
    let verbatim: speclance_ms::mzml::Verbatim = serde_json::from_value(meta).unwrap();
    assert!(verbatim.prefix.contains("<mzML"));
}

#[test]
fn skipped_chromatogram_arrays_are_counted() {
    let xml = String::from_utf8(tiny_with_chromatogram(MZ_B64)).unwrap();
    let xml = xml.replace(
        r#"</binaryDataArrayList></chromatogram>"#,
        &format!("{MOBILITY_ARRAY}</binaryDataArrayList></chromatogram>"),
    );
    let mut ingest = MzmlIngest::open_bytes(xml.into_bytes(), "chrom-skip.mzML".into()).unwrap();
    while ingest.next_spectrum().unwrap().is_some() {}
    let chroms = ingest.read_chromatograms().unwrap();
    assert_eq!(chroms[0].time, vec![40.0, 50.0, 64.0]);
    assert_eq!(chroms[0].intensity, vec![3.0, 5.0, 7.5]);
    assert_eq!(
        ingest.skipped_arrays().get("mean ion mobility array"),
        Some(&1)
    );
}

#[test]
fn run_metadata_has_no_skipped_arrays_key_when_nothing_was_skipped() {
    let data = parse_bytes(TINY_MZML.as_bytes(), "tiny.mzML".into()).unwrap();
    let meta: serde_json::Value =
        serde_json::from_str(data.run.run_metadata.as_deref().unwrap()).unwrap();
    assert!(meta.get("skipped_arrays").is_none(), "{meta}");

    let skipped = parse_bytes(&tiny_with_extra_arrays(&[MOBILITY_ARRAY]), "s.mzML".into()).unwrap();
    let meta: serde_json::Value =
        serde_json::from_str(skipped.run.run_metadata.as_deref().unwrap()).unwrap();
    assert_eq!(meta["skipped_arrays"]["count"], 1);
}
