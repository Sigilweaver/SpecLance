# prolance (Python)

Python bindings for [ProLance](https://github.com/Sigilweaver/ProLance),
a Lance-backed mass-spectrometry data store.

## Install (dev)

From the workspace root:

```sh
cd crates/prolance-py
maturin develop --release
```

## Usage

```python
import prolance

store = prolance.Store.open("/tmp/plstore")
run_id = store.ingest_mzml("path/to/run.mzML")

# Build scalar indexes once after a bulk ingest.
store.create_default_indexes()

runs = store.runs()
print(runs[0]["run_id"], runs[0]["spectrum_count"])

# RT window query, MS1 only, first 50 hits.
hits = store.query_window(run_id, rt_min=0.0, rt_max=120.0,
                          ms_level=1, limit=50)
for h in hits:
    print(h["scan_num"], h["rt"], len(h["mz"]))

chroms = store.chromatograms(run_id)
```
