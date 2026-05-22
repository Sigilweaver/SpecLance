# ProLance

> Part of the [OpenProteo](https://sigilweaver.app/openproteo/docs/)
> stack for proteomics raw-file access. ProLance is the columnar
> storage layer; vendor readers
> [OpenWRaw](https://github.com/Sigilweaver/OpenWRaw),
> [OpenTFRaw](https://github.com/Sigilweaver/OpenTFRaw), and
> [OpenTimsTDF](https://github.com/Sigilweaver/OpenTimsTDF) feed it
> through [openproteo-io](https://github.com/Sigilweaver/OpenProteo).

A fast, columnar, memory-mapped mass spectrometry data store powered by [Lance].

ProLance is a **storage format**, not an analysis framework. Its purpose is to
serve as a drop-in replacement for mzML in existing proteomics workflows - one
that is faster to read, cheaper to seek, and can hold many runs in a single
directory. Researchers should be able to swap `run.mzML` for a ProLance store,
retrieve spectra using familiar Python or Rust objects, and feed them directly
into existing libraries without rewriting any analysis code.

## Vision

```
                       ingest
   .raw (Thermo)  ─────────────┐
   .d/  (Bruker)  ─────────────┤
   .raw (Waters)  ─────────────┼──►  ProLance store  ──►  analysis libraries
   .mzML          ─────────────┘         │               (spectrum_utils,
                                         │                matchms, pyOpenMS,
                                    export │                ms2ml, ...)
                                         ▼
                                    .mzML (roundtrip)
```

There are three usage modes:

1. **Ingest from vendor format.** Use the open reader crates (OpenTFRaw,
   OpenTimsTDF, OpenWRaw) to parse proprietary binary files and write a ProLance
   store. No vendor SDK, no Wine, no COM interop.
2. **Ingest from mzML.** Parse existing mzML files and write a ProLance
   store. This is the zero-friction on-ramp for labs that already have mzML
   pipelines.
3. **Export to mzML.** Reconstruct a spec-valid mzML file from a ProLance
   store. The roundtrip `mzML -> ProLance -> mzML` must yield an effectively
   identical file: same spectra, same CV terms, same metadata, same ordering.
   This invariant is what makes ProLance safe to insert into existing
   pipelines without data loss.

## Why Not mzML?

mzML is the HUPO-PSI community standard and it is well-supported, but it has
structural properties that make it expensive at scale:

- It is XML, which means sequential parsing. Seeking to scan 50,000 out of
  200,000 requires either re-parsing from the start or maintaining an index
  file (mzML.index / mzMLb).
- Every peak array is Base64-encoded inline in the XML, which inflates size
  and requires a decode step before any numerical work.
- Cross-run queries (e.g., "all MS2 spectra with precursor 445.22 +/- 10 ppm
  across 40 LC-MS runs") require loading and parsing each file independently.
- Profile-mode Orbitrap runs routinely exceed 1 GB. TimsTOF PASEF runs can
  reach 10-20 GB per acquisition.

Lance stores data as Apache Arrow columns in memory-mapped files. Filtering
by retention time, MS level, or precursor m/z touches only the relevant
columns, not the peak arrays. A region scan that would require parsing an
entire mzML file can be answered by reading a fraction of the bytes.

## Architecture

The workspace will follow the same three-crate pattern as BioLance:

```
prolance-core/     Arrow schema definitions, LanceDB table management,
                   shared types (Spectrum, Chromatogram, Run).

prolance-ms/       Ingest adapters.
                     - mzML reader (noodles or quick-xml + mzml CV table)
                     - OpenTFRaw adapter (Thermo .raw)
                     - OpenTimsTDF adapter (Bruker timsTOF .d)
                     - OpenWRaw adapter (Waters MassLynx .raw)
                   Each adapter normalises its source into the common
                   prolance_core::Spectrum type.

prolance-cli/      `prolance` binary.
                     ingest   - write a ProLance store from any source
                     query    - filter spectra by RT / MS level / precursor
                     xic      - extracted ion chromatogram across runs
                     compare  - cross-run TIC / XIC overlays
                     export   - reconstruct mzML from a ProLance store
```

A Python package (`prolance-py`, built with PyO3 + maturin) will expose the
store via a thin API that returns objects compatible with common analysis
libraries.

## Schema

### `runs` table

One row per ingested file.

| column           | Arrow type | notes                                   |
|------------------|------------|-----------------------------------------|
| run_id           | Utf8       | SHA-256 of the source file path + mtime |
| source_path      | Utf8       |                                         |
| source_format    | Utf8       | "mzml", "thermo", "bruker", "waters"    |
| instrument       | Utf8       | instrument model name from CV / metadata|
| start_time       | Timestamp  |                                         |
| spectrum_count   | UInt32     |                                         |
| ms1_count        | UInt32     |                                         |
| ms2_count        | UInt32     |                                         |
| run_metadata     | Utf8 (JSON)| full mzML run-level CV params + attrs   |

### `spectra` table

One row per spectrum. Peak data lives in list-typed columns so each spectrum
is self-contained and can be retrieved atomically.

| column                   | Arrow type          | notes                             |
|--------------------------|---------------------|-----------------------------------|
| run_id                   | Utf8                |                                   |
| scan_num                 | UInt32              | 1-based, matches source           |
| native_id                | Utf8                | mzML nativeID string              |
| ms_level                 | UInt8               | 1, 2, ...                         |
| rt                       | Float64             | seconds                           |
| tic                      | Float64             | total ion current                 |
| base_peak_mz             | Float64             |                                   |
| base_peak_intensity      | Float64             |                                   |
| polarity                 | Int8                | +1 / -1                           |
| centroided               | Boolean             | centroid vs. profile mode         |
| precursor_mz             | Float64             | null for MS1                      |
| precursor_charge         | Int8                | null for MS1                      |
| precursor_intensity      | Float64             | null for MS1                      |
| isolation_window_lower   | Float64             | null for MS1                      |
| isolation_window_upper   | Float64             | null for MS1                      |
| activation               | Utf8                | HCD / CID / ECD / ETD / ...       |
| collision_energy         | Float32             |                                   |
| inv_mobility             | Float64             | Bruker 1/K0; null for other vendors|
| mz_precision             | UInt8               | 32 or 64 - original storage width |
| intensity_precision      | UInt8               | 32 or 64 - original storage width |
| mz                       | LargeList<Float64>  | peak m/z values                   |
| intensity                | LargeList<Float32>  | peak intensity values             |
| cv_params                | Utf8 (JSON)         | all spectrum-level CV params      |
| scan_window_lower        | Float64             |                                   |
| scan_window_upper        | Float64             |                                   |

### `chromatograms` table

mzML documents routinely include chromatogram traces (TIC, BPC, SRM/MRM
transitions). These are structurally separate from spectra in the mzML schema
and must be preserved for a lossless roundtrip.

| column         | Arrow type         | notes                                  |
|----------------|--------------------|----------------------------------------|
| run_id         | Utf8               |                                        |
| chrom_id       | Utf8               | mzML id attribute                      |
| chrom_type     | Utf8               | TIC / BPC / SRM / ...                  |
| precursor_mz   | Float64            | null for TIC/BPC                       |
| product_mz     | Float64            | null for TIC/BPC                       |
| time           | LargeList<Float32> | time axis (seconds)                    |
| intensity      | LargeList<Float32> | intensity axis                         |
| cv_params      | Utf8 (JSON)        | all chromatogram-level CV params       |

## Challenges and How to Address Them

### 1. CV term completeness

mzML is governed by the PSI-MS OBO ontology, which defines over 1,000
controlled vocabulary terms covering instrument components, scan types, data
transformations, activation methods, and more. Any schema that hard-codes only
the common terms will silently drop the rest on ingest, making a lossless
roundtrip impossible.

**Approach:** Use a hybrid strategy. Map the ~30 terms that researchers
routinely query (ms level, RT, precursor m/z, charge, activation, polarity,
centroid flag) to typed Arrow columns so they are fast to filter. Store every
other CV param for the spectrum as a JSON string in the `cv_params` column.
On mzML export, the typed columns are re-emitted as CV params with their
correct accession numbers, and the `cv_params` blob is written out verbatim.
Nothing is lost even for obscure instrument-specific terms.

The same pattern applies at the run level: the full `<fileDescription>`,
`<sampleList>`, `<softwareList>`, `<scanSettingsList>`,
`<instrumentConfigurationList>`, and `<dataProcessingList>` blocks are stored
as JSON in `runs.run_metadata` and written back character-for-character on
export.

### 2. Binary precision preservation

mzML binary arrays can be stored as 32-bit or 64-bit IEEE 754 floats, and
some files use MS-Numpress lossless or lossy integer encodings. Internally,
ProLance stores all m/z values as Float64 (sufficient for any precision),
but the `mz_precision` and `intensity_precision` columns record the original
bit width. On export, values are downcast to the original width before
Base64-encoding, preserving the roundtrip value to within the original
precision without storing redundant 32-bit copies.

MS-Numpress re-encoding is deterministic for the lossless variants (Linear
and Pic), so re-applying the same algorithm on export will yield identical
encoded bytes. If the source file used Numpress, the `cv_params` blob will
record that fact and the exporter will apply the matching encoder.

### 3. Ion mobility (4D data from Bruker TIMS)

Bruker timsTOF PASEF acquisitions add a full ion mobility dimension. Each
"frame" in the TDF format is a collection of scans across the TIMS ramp, each
scan being a TOF spectrum at a specific inverse-mobility (1/K0) value. This
does not fit the flat mzML spectrum model cleanly.

mzML 1.1.3+ defines a convention for ion mobility (CV term MS:1002476 for
mean 1/K0, MS:1003008 for scan-level IM arrays), but support across readers
is inconsistent. There are two representation options:

- **Collapsed (recommended for most workflows):** Sum or select peaks across
  the mobility dimension at ingest, producing conventional MS1/MS2 spectra
  with an `inv_mobility` centroid value. Matches what most downstream tools
  expect.
- **Expanded (full-fidelity):** Store one row per TIMS scan, with an
  `inv_mobility` value per row. The `scan_num` + `run_id` + `inv_mobility`
  tuple uniquely identifies each record. This preserves the full 4D dataset
  at the cost of row count.

ProLance should support both modes as an ingest flag (`--tims-mode
collapsed|expanded`). The mzML exporter will use the expanded-mode
representation when exporting TIMS data.

### 4. Chromatograms

mzML `<chromatogramList>` is routinely populated by instruments with TIC,
BPC, and SRM/MRM transition traces. It is separate from the spectrum list
and cannot be mapped into the spectra table without information loss.

The `chromatograms` table above addresses this directly. On mzML ingest,
chromatograms are parsed and written to that table. On export, they are
reconstructed and written as a `<chromatogramList>` block. Any tool that
expects chromatograms in its mzML output will still find them.

### 5. Data processing lineage

mzML documents record what software transformations have been applied to the
data (centroiding, deisotoping, denoising, calibration) in a
`<dataProcessingList>`. This block affects how consumers interpret the data
(e.g., whether to apply their own centroiding) and must be preserved.

The `run_metadata` JSON column stores the full `<dataProcessingList>` XML
fragment. The exporter writes it back verbatim. If ProLance itself transforms
data during ingest (e.g., vendor-to-mzML conversion adds a processing step),
a new `<processingMethod>` entry is appended recording the conversion.

### 6. Vendor metadata beyond mzML

The three vendor readers expose metadata that does not have a direct mzML CV
term mapping: Thermo filter strings (`FTMS + p ESI Full ms [300.00-1500.00]`),
Waters function types, Bruker accumulation times. This information is useful
for debugging and for re-creating accurate mzML output.

Each vendor adapter will serialize its source-specific scan metadata into the
`cv_params` JSON column using a `vendor:` namespace prefix for fields with no
PSI-MS accession. On mzML export, fields with a real accession are emitted as
`<cvParam>` elements; vendor-namespaced fields are emitted as `<userParam>`
elements. mzML allows `<userParam>` anywhere CV params appear, so no
information is lost and the file remains schema-valid.

### 7. Library interoperability (Python)

The primary consumer surface is Python. The most widely-used spectrum
libraries - `spectrum_utils`, `matchms`, `ms2ml`, `pyOpenMS`, `pyteomics` -
each define their own spectrum object. None of them read Lance natively.

The `prolance-py` Python package will expose:

```python
import prolance

store = prolance.open("runs/")

# Iterator of spectrum_utils.spectrum.MsmsSpectrum objects
for spec in store.ms2_spectra(run="run01", rt_min=10.0, rt_max=20.0):
    ...

# matchms.Spectrum objects
for spec in store.as_matchms(run="run01"):
    ...

# Direct numpy arrays for custom code
mz, intensity = store.peaks(run="run01", scan=5432)
```

The adapters are thin wrappers that copy the numpy arrays from the Arrow
columns directly into the target library's data structures, with zero
additional parsing.

### 8. Large file streaming

A single Orbitrap run can be 1-2 GB of mzML; a full timsTOF PASEF experiment
can exceed 20 GB. Loading everything into RAM before writing is not feasible.

Ingest will stream spectra in batches (default 10,000 spectra per batch) and
write each batch as a Lance fragment. Lance's columnar chunking means each
fragment is independently readable; no seek across the whole store is needed
for a range query. The CLI will report progress via an `indicatif` progress
bar, matching the BioLance pattern.

### 9. Roundtrip correctness as a first-class invariant

The mzML -> ProLance -> mzML roundtrip is not an afterthought - it is the
primary correctness guarantee that makes ProLance safe to deploy as an
infrastructure swap. It should be tested with a suite of real mzML files
spanning:

- Thermo Orbitrap (DDA and DIA)
- Bruker timsTOF PASEF
- Waters HDMS (ion mobility)
- SRM/MRM files with chromatogram lists
- Files with both 32-bit and 64-bit arrays
- Files with Numpress-encoded arrays
- Files with unusual or instrument-vendor-specific CV terms

The test suite will parse the exported mzML with an independent parser
(e.g., `mzdata` or `psims`) and assert equality of spectrum count, CV params,
retention times, precursor values, and peak arrays to within floating-point
precision of the original storage width.

## Dependency Map

```
prolance-core
  lancedb, arrow-array, arrow-schema, serde_json, anyhow, thiserror

prolance-ms
  prolance-core
  opentfraw   (Thermo .raw)
  OpenTimsTDF     (Bruker timsTOF .d)
  openwraw    (Waters .raw)
  quick-xml   (mzML ingest)
  base64      (binary array decoding)

prolance-cli
  prolance-core, prolance-ms
  clap, tokio, rayon, indicatif

prolance-py  (separate maturin crate)
  prolance-core, prolance-ms
  pyo3, numpy
```

No vendor SDK, no Wine, no COM server, no proprietary runtime dependency of
any kind.

## Relation to Existing Projects

| project    | role                                              |
|------------|---------------------------------------------------|
| OpenTFRaw  | Thermo .raw parser; ingest adapter source         |
| OpenTimsTDF    | Bruker timsTOF parser; ingest adapter source      |
| OpenWRaw   | Waters MassLynx parser; ingest adapter source     |
| BioLance   | Conceptual template (VCF -> Lance); sibling project|
| ProLance   | Storage layer; delegates parsing to the above     |

[Lance]: https://lancedb.github.io/lance/
