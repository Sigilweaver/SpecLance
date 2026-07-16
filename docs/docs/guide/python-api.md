---
sidebar_position: 1
---

# Python API

The `speclance` wheel (built with PyO3 + maturin) exposes a synchronous
`Store` facade over the Rust core. Each call blocks on an internal Tokio
runtime, so no `async`/`await` is required on the Python side. Install it
with `pip install speclance`.

```python
import speclance

store = speclance.Store.open("./run-store")
```

`Store.open` opens an existing store or creates a new one at `path` if it
does not exist yet.

## `Store`

| Member                        | Type          | Description                                                        |
| ------------------------------ | ------------- | -------------------------------------------------------------------- |
| `Store.open(path)`             | staticmethod  | Open (or create) a store at `path`.                                 |
| `ingest_mzml(path)`            | `str`         | Stream-ingest an `.mzML` file. Returns the resulting `run_id`.       |
| `create_default_indexes()`     | `None`        | Build the default scalar indexes on the `spectra`/`runs` tables.     |
| `runs()`                       | `list[dict]`  | List runs in the store.                                              |
| `query_window(...)`            | `list[dict]`  | Range-query spectra by run, retention time, and MS level.            |
| `chromatograms(run_id)`        | `list[dict]`  | Fetch chromatograms for a run.                                       |

Spectra, runs, and chromatograms are all returned as plain Python `dict`
objects (not a custom class), so the binding has no hard dependency on
pyarrow at import time.

### `ingest_mzml(path)`

Streams an `.mzML` file into the store using the same streaming reader as
the CLI, then appends the resulting run and any chromatograms.

```python
run_id = store.ingest_mzml("path/to/run.mzML")
```

### `create_default_indexes()`

Builds the default scalar indexes (used to accelerate `query_window`).
Call this once after ingesting, before querying:

```python
store.create_default_indexes()
```

### `runs()`

Returns a list of dicts, one per ingested run, with these keys: `run_id`,
`source_path`, `source_format`, `instrument`, `start_time`,
`spectrum_count`, `ms1_count`, `ms2_count`.

```python
for run in store.runs():
    print(run["run_id"], run["spectrum_count"])
```

### `query_window(run_id, rt_min=None, rt_max=None, ms_level=None, limit=None)`

Range-queries spectra for `run_id` in the given retention-time window
(seconds), optionally filtered by MS level and capped at `limit` results.
Omitted bounds are unbounded (`rt_min`/`rt_max` default to -inf/+inf).

Returns a list of dicts with keys: `run_id`, `scan_num`, `ms_level`, `rt`,
`mz` (`list[float]`), `intensity` (`list[float]`), `tic`, `precursor_mz`,
`native_id`.

```python
runs = store.runs()
hits = store.query_window(
    run_id=runs[0]["run_id"],
    rt_min=30.0,
    rt_max=35.0,
    ms_level=1,
)
for spectrum in hits:
    mz, intensity = spectrum["mz"], spectrum["intensity"]
```

### `chromatograms(run_id)`

Returns a list of dicts, one per chromatogram in `run_id`, with keys:
`run_id`, `chrom_id`, `chrom_type`, `precursor_mz`, `product_mz`, `time`
(`list[float]`), `intensity` (`list[float]`).

```python
for chrom in store.chromatograms(run_id):
    print(chrom["chrom_id"], len(chrom["time"]))
```

## Next

- [GitHub](https://github.com/Sigilweaver/SpecLance) for the CLI and the
  on-disk schema.
