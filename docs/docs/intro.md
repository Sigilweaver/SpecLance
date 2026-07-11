---
slug: /
title: Introduction
---

# SpecLance

SpecLance is a columnar, memory-mapped mass-spectrometry store built on
[Lance](https://lancedb.github.io/lance/). It ingests vendor formats
(via [OpenMassSpec](https://github.com/Sigilweaver/OpenMassSpec)) or mzML
and exposes a queryable store from both Rust and Python with direct
PyArrow / Polars / Pandas integration.

This is the 0.2.0-alpha release. APIs and the on-disk schema may break
without notice until 1.0.

## What is in the box

| Component        | Purpose                                                       |
| ---------------- | ------------------------------------------------------------- |
| `speclance-core`  | Lance store, schema, scalar indexes, range-query API.         |
| `speclance-ms`    | mzML reader/writer, streaming ingest, vendor feature gates.   |
| `speclance-cli`   | `speclance` binary: ingest, inspect, query, export back to mzML. |
| `speclance` (PyPI) | Python bindings (PyO3) backed by `speclance-core` + `speclance-ms`. |

## Stack position

SpecLance sits at the storage layer of the OpenMassSpec stack:

```
vendor file (Thermo .raw, Bruker .d/, Waters .raw/, or .mzML)
   |
   v
openmassspec-io  (vendor parsing, all of it - SpecLance never touches
   |            vendor formats directly)
   v
speclance-ms / speclance-core  (Lance dataset, indexed by RT and m/z)
   |
   v
queries from Rust, Python, or the CLI
```

## Quickstart

The CLI is the fastest way to try it out:

```bash
cargo install --git https://github.com/Sigilweaver/SpecLance speclance-cli
speclance ingest path/to/spectra.mzML --store ./run-store
speclance query ./run-store --rt 30..35 --mz 500..510
```

Python:

```bash
pip install speclance
```

```python
from speclance import Store
store = Store.open("./run-store")
batch = store.query(rt=(30, 35), mz=(500, 510))
```

See [GitHub](https://github.com/Sigilweaver/SpecLance) for the current
roadmap and contribution guidelines.
