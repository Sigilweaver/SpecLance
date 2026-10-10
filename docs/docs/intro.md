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
| `speclance-cli`   | `speclance` binary: ingest, inspect, query, export back to mzML. |
| `speclance` (PyPI) | Python bindings (PyO3) backed by `speclance-core` + `speclance-ms`. |

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

## Known limitations

- mzML export is lossy: it writes spectra only, without chromatograms,
  extra CV params, or run-level metadata. Keep your source files.
- MS-Numpress encoded arrays are not supported; ingest fails on them.
- For mzML ingest, `scan_num` is the spectrum index + 1, not the source
  scan number. The nativeID is kept in `native_id`.
- Re-ingesting a run already in the store is refused.
- No m/z index or m/z query yet.

## Quickstart

The CLI is the fastest way to try it out:

```bash
cargo install --git https://github.com/Sigilweaver/SpecLance speclance-cli
speclance ingest --store ./run-store path/to/spectra.mzML
speclance index --store ./run-store
speclance runs --store ./run-store
speclance query --store ./run-store --run-id <run_id> --rt-min 30 --rt-max 35
```

Python:

```bash
pip install speclance
```

```python
from speclance import Store
store = Store.open("./run-store")
run_id = store.ingest_mzml("path/to/spectra.mzML")
hits = store.query_window(run_id, rt_min=30.0, rt_max=35.0)
```

See the [Python API](./guide/python-api.md) page for details.

See [GitHub](https://github.com/Sigilweaver/SpecLance) for the current
roadmap and contribution guidelines.
