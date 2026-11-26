"""Round-trip tests for the speclance Python bindings.

These build a tiny in-memory mzML, write it to disk, ingest into a
SpecLance store, and exercise indexing + range queries.
"""
from __future__ import annotations

import os
import tempfile

import pytest

import speclance


TINY_MZML = """<?xml version="1.0" encoding="utf-8"?>
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
"""


def test_ingest_index_query_roundtrip(tmp_path):
    mzml_path = tmp_path / "tiny.mzML"
    mzml_path.write_text(TINY_MZML)

    store = speclance.Store.open(str(tmp_path / "store"))
    run_id = store.ingest_mzml(str(mzml_path))
    assert isinstance(run_id, str) and run_id

    store.create_default_indexes()
    # Idempotent.
    store.create_default_indexes()

    runs = store.runs()
    assert len(runs) == 1
    assert runs[0]["run_id"] == run_id
    assert runs[0]["spectrum_count"] == 1
    assert runs[0]["ms1_count"] == 1

    # The synthetic spectrum has rt=1.5, ms_level=1, 3 peaks.
    hits = store.query_window(run_id, rt_min=0.0, rt_max=10.0, ms_level=1)
    assert len(hits) == 1
    h = hits[0]
    assert h["scan_num"] == 1
    assert h["ms_level"] == 1
    assert abs(h["rt"] - 1.5) < 1e-6
    assert len(h["mz"]) == 3
    assert len(h["intensity"]) == 3
    assert abs(h["tic"] - 300.5) < 1e-3

    # Out-of-window query returns nothing.
    assert store.query_window(run_id, rt_min=100.0, rt_max=200.0) == []

    # MS level mismatch returns nothing.
    assert store.query_window(run_id, ms_level=2) == []


def test_store_repr(tmp_path):
    store = speclance.Store.open(str(tmp_path / "store"))
    assert "speclance.Store" in repr(store)


def test_reingest_same_run_is_rejected(tmp_path):
    mzml_path = tmp_path / "tiny.mzML"
    mzml_path.write_text(TINY_MZML)

    store = speclance.Store.open(str(tmp_path / "store"))
    run_id = store.ingest_mzml(str(mzml_path))
    with pytest.raises(OSError, match="already in this store"):
        store.ingest_mzml(str(mzml_path))

    runs = store.runs()
    assert len(runs) == 1
    assert len(store.query_window(run_id)) == 1


def _many_spectra_mzml(count, last_index):
    """TINY_MZML with `count` copies of its spectrum; the last one has
    `last_index` as its index attribute."""
    start = TINY_MZML.index("      <spectrum index=")
    end = TINY_MZML.index("</spectrum>\n") + len("</spectrum>\n")
    block = TINY_MZML[start:end]
    spectra = []
    for i in range(count):
        index = last_index if i + 1 == count else str(i)
        spectra.append(
            block.replace('index="0" id="scan=1"', f'index="{index}" id="scan={i + 1}"')
        )
    head = TINY_MZML[:start].replace('<spectrumList count="1"', f'<spectrumList count="{count}"')
    return head + "".join(spectra) + TINY_MZML[end:]


def test_failed_ingest_leaves_no_rows_and_the_file_can_be_retried(tmp_path):
    mzml_path = tmp_path / "run.mzML"
    store = speclance.Store.open(str(tmp_path / "store"))

    # 2050 spectra: the first write batch (2048) is stored before the bad
    # index of the last spectrum is reached.
    mzml_path.write_text(_many_spectra_mzml(2050, "x"))
    with pytest.raises(OSError, match="spectrum index"):
        store.ingest_mzml(str(mzml_path))
    assert store.runs() == []

    # Same path and byte length, so the same run_id.
    mzml_path.write_text(_many_spectra_mzml(2050, "9"))
    run_id = store.ingest_mzml(str(mzml_path))
    assert len(store.runs()) == 1
    assert len(store.query_window(run_id)) == 2050


def test_ingest_reports_skipped_arrays_on_stderr(tmp_path, capfd):
    # An mzML spectrum with a mean ion mobility array: only m/z and
    # intensity are stored, and the skipped array is counted on stderr.
    mobility = """<binaryDataArray encodedLength="16">
            <cvParam cvRef="MS" accession="MS:1000521" name="32-bit float"/>
            <cvParam cvRef="MS" accession="MS:1000576" name="no compression"/>
            <cvParam cvRef="MS" accession="MS:1002816" name="mean ion mobility array"/>
            <binary>AABAQAAAoEAAAPBA</binary>
          </binaryDataArray>
        </binaryDataArrayList>"""
    xml = TINY_MZML.replace("</binaryDataArrayList>", mobility, 1)
    mzml_path = tmp_path / "mobility.mzML"
    mzml_path.write_text(xml)

    store = speclance.Store.open(str(tmp_path / "store"))
    run_id = store.ingest_mzml(str(mzml_path))
    err = capfd.readouterr().err
    assert (
        "skipped 1 binary data arrays that are not m/z or intensity "
        "(mean ion mobility array: 1)" in err
    ), err
    assert len(store.query_window(run_id)) == 1


def test_ingest_without_skipped_arrays_reports_none(tmp_path, capfd):
    mzml_path = tmp_path / "tiny.mzML"
    mzml_path.write_text(TINY_MZML)
    store = speclance.Store.open(str(tmp_path / "store"))
    store.ingest_mzml(str(mzml_path))
    assert "skipped" not in capfd.readouterr().err
