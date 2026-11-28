---
slug: /
title: Introduction
---

# SpecLance

SpecLance is a columnar, memory-mapped mass-spectrometry store built on
[Lance](https://lancedb.github.io/lance/). It ingests vendor formats
(via [OpenMassSpec](https://github.com/Sigilweaver/OpenMassSpec)) or mzML
and exposes a queryable store from Rust, Python, and the CLI. The Python
API returns plain `dict` and `list` objects.

This is the 0.2.0-alpha release. APIs and the on-disk schema may break
without notice until 1.0.

## What is in the box

| Component        | Purpose                                                       |
| ---------------- | ------------------------------------------------------------- |
| `speclance-core`  | Lance store, schema, scalar indexes, RT-window query API.     |
| `speclance-ms`    | mzML reader/writer, streaming ingest, vendor feature gates.   |
| `speclance-cli`   | `speclance` binary: ingest, index, list runs, query, export to mzML. |
| `speclance` (Python) | Python bindings (PyO3) backed by `speclance-core` + `speclance-ms`. |

## Stack position

SpecLance sits at the storage layer of the OpenMassSpec stack:

```
vendor file (Thermo, Bruker, Waters, Agilent, SCIEX, Shimadzu) or .mzML
   |
   v
openmassspec-io  (vendor parsing, all of it - SpecLance never touches
   |            vendor formats directly)
   v
speclance-ms / speclance-core  (Lance tables, indexed by run, scan, RT, MS level)
   |
   v
queries from Rust, Python, or the CLI
```

Vendor ingest is off by default. Build the CLI with
`--features all-vendors` to enable it. The Python package ingests mzML
only.

## Known issues

- mzML export is lossy: it writes spectra only, without chromatograms,
  extra CV params, or run-level metadata. Keep your source files.
- MS-Numpress encoded arrays, in spectra or chromatograms, are not supported; ingest fails on them.
- Only m/z and intensity arrays are stored. Ingest counts the other binary
  data arrays it skips and reports the count on stderr.
- For mzML ingest, `scan_num` is the spectrum index + 1, not the source
  scan number. The nativeID is kept in `native_id`.
- `run_id` is derived from the source path and file size, so the same file
  reached by a different path gets a different `run_id`.
- The mzML activation names `SID`, `HCD`, `EThcD`, `ETD`, `MPD` and `MPID` do not
  all roundtrip; the README lists each case.
- Queries filter on run, retention time, and MS level. There is no m/z
  query and no cross-run query.

The README on [GitHub](https://github.com/Sigilweaver/SpecLance) lists all
known issues, including the mzML activation cases that do not roundtrip.

## Quickstart

Build the CLI from source:

```bash
git clone https://github.com/Sigilweaver/SpecLance
cd SpecLance
cargo build --release -p speclance-cli
export PATH="$PWD/target/release:$PATH"
```

Ingest an mzML file, index the store, and query a retention-time window
(seconds) of one run:

```bash
speclance ingest --store ./run-store path/to/spectra.mzML
speclance index --store ./run-store
speclance runs --store ./run-store
speclance query --store ./run-store --run-id <run_id> --rt-min 30 --rt-max 35
speclance export --store ./run-store --run-id <run_id> --out out.mzML
```

`speclance runs` prints the `run_id` of each ingested run.

Python (3.9 or newer), built from the same checkout:

```bash
pip install maturin
cd crates/speclance-py
maturin develop --release
```

```python
from speclance import Store

store = Store.open("./run-store")
run_id = store.ingest_mzml("path/to/spectra.mzML")
hits = store.query_window(run_id, rt_min=30.0, rt_max=35.0)
```

See the [Python API](./guide/python-api.md) page for details.

See [GitHub](https://github.com/Sigilweaver/SpecLance) for contribution
guidelines.
