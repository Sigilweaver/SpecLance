"""SpecLance: Lance-backed mass-spectrometry data store.

Synchronous Python facade over the Rust core. Each call blocks on
an internal Tokio runtime.

Example:

    import speclance
    store = speclance.Store.open("/tmp/plstore")
    store.ingest_mzml("path/to/run.mzML")
    runs = store.runs()
    hits = store.query_window(run_id=runs[0]["run_id"],
                              rt_min=0.0, rt_max=120.0,
                              ms_level=1)
"""
from __future__ import annotations

from ._speclance import Store, __version__  # noqa: F401

__all__ = ["Store", "__version__"]
