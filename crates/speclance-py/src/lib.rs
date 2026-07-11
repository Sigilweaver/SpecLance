//! PyO3 bindings for SpecLance.
//!
//! Exposes a synchronous [`Store`] facade that blocks on an internal
//! Tokio runtime. Spectra are returned as plain Python dicts so the
//! binding does not require pyarrow at import time.

// pyo3 0.22's pymethods macros emit `into()` calls that clippy flags
// as `useless_conversion` for some return types. Silence at the crate
// level rather than peppering attributes on every method.
#![allow(clippy::useless_conversion)]

use std::path::Path;
use std::sync::Arc;

use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use speclance_core::{
    batches_to_chromatograms, batches_to_runs, batches_to_spectra, Store as CoreStore,
};
use speclance_ms::mzml::MzmlIngest;
use tokio::runtime::Runtime;

const SPECTRUM_CHUNK: usize = 2048;

fn map_err<E: std::fmt::Display>(e: E) -> PyErr {
    PyIOError::new_err(e.to_string())
}

/// Lance-backed mass-spectrometry store.
#[pyclass(module = "speclance._speclance", name = "Store")]
struct PyStore {
    rt: Arc<Runtime>,
    inner: Arc<CoreStore>,
    path: String,
}

#[pymethods]
impl PyStore {
    /// Open (or create) a SpecLance store at `path`.
    #[staticmethod]
    fn open(path: &str) -> PyResult<Self> {
        let rt = Runtime::new().map_err(map_err)?;
        let store = rt.block_on(CoreStore::open(path)).map_err(map_err)?;
        Ok(Self {
            rt: Arc::new(rt),
            inner: Arc::new(store),
            path: path.to_string(),
        })
    }

    fn __repr__(&self) -> String {
        format!("<speclance.Store path={:?}>", self.path)
    }

    /// Ingest a single `.mzML` file using the streaming reader.
    /// Returns the resulting `run_id`.
    fn ingest_mzml(&self, path: &str) -> PyResult<String> {
        let store = Arc::clone(&self.inner);
        let p = path.to_string();
        self.rt.block_on(async move {
            let mut ingest = MzmlIngest::open(Path::new(&p))
                .map_err(|e| PyIOError::new_err(format!("open mzml: {e}")))?;
            let run_id = ingest.run_id().to_string();
            let mut buf = Vec::with_capacity(SPECTRUM_CHUNK);
            let mut total = 0u32;
            let mut ms1 = 0u32;
            let mut ms2 = 0u32;
            while let Some(s) = ingest
                .next_spectrum()
                .map_err(|e| PyIOError::new_err(format!("parse spectrum: {e}")))?
            {
                if s.ms_level == 1 {
                    ms1 += 1;
                } else if s.ms_level >= 2 {
                    ms2 += 1;
                }
                total += 1;
                buf.push(s);
                if buf.len() >= SPECTRUM_CHUNK {
                    store.append_spectra(&buf).await.map_err(map_err)?;
                    buf.clear();
                }
            }
            if !buf.is_empty() {
                store.append_spectra(&buf).await.map_err(map_err)?;
            }
            let chromatograms = ingest
                .read_chromatograms()
                .map_err(|e| PyIOError::new_err(format!("parse chromatograms: {e}")))?;
            let run = ingest
                .finalize_run(total, ms1, ms2)
                .map_err(|e| PyIOError::new_err(format!("finalize run: {e}")))?;
            store.append_run(&run).await.map_err(map_err)?;
            if !chromatograms.is_empty() {
                store
                    .append_chromatograms(&chromatograms)
                    .await
                    .map_err(map_err)?;
            }
            Ok::<_, PyErr>(run_id)
        })
    }

    /// Build the default scalar indexes on spectra/runs tables.
    fn create_default_indexes(&self) -> PyResult<()> {
        let store = Arc::clone(&self.inner);
        self.rt
            .block_on(async move { store.create_default_indexes().await })
            .map_err(map_err)
    }

    /// List runs in the store. Returns a list of dicts with run_id,
    /// source_format, instrument, spectrum_count.
    fn runs<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let store = Arc::clone(&self.inner);
        let batches = self
            .rt
            .block_on(async move {
                let names = store.table_names().await?;
                if !names.iter().any(|n| n == "runs") {
                    return Ok::<_, speclance_core::Error>(Vec::new());
                }
                use futures::TryStreamExt;
                use lancedb::query::ExecutableQuery;
                let table = store.runs_table().await?;
                let stream = table.query().execute().await?;
                let batches: Vec<_> = stream.try_collect().await?;
                Ok(batches)
            })
            .map_err(map_err)?;
        let runs = batches_to_runs(&batches);
        let list = PyList::empty_bound(py);
        for r in runs {
            let d = PyDict::new_bound(py);
            d.set_item("run_id", r.run_id)?;
            d.set_item("source_path", r.source_path)?;
            d.set_item("source_format", r.source_format)?;
            d.set_item("instrument", r.instrument)?;
            d.set_item("start_time", r.start_time)?;
            d.set_item("spectrum_count", r.spectrum_count)?;
            d.set_item("ms1_count", r.ms1_count)?;
            d.set_item("ms2_count", r.ms2_count)?;
            list.append(d)?;
        }
        Ok(list)
    }

    /// Range-query spectra in a retention-time window. Returns a list
    /// of dicts (scan_num, ms_level, rt, mz, intensity, tic,
    /// precursor_mz). `mz` and `intensity` are returned as Python
    /// lists of floats.
    #[pyo3(signature = (run_id, rt_min=None, rt_max=None, ms_level=None, limit=None))]
    fn query_window<'py>(
        &self,
        py: Python<'py>,
        run_id: &str,
        rt_min: Option<f64>,
        rt_max: Option<f64>,
        ms_level: Option<u8>,
        limit: Option<usize>,
    ) -> PyResult<Bound<'py, PyList>> {
        let store = Arc::clone(&self.inner);
        let run_id_owned = run_id.to_string();
        let batches = self
            .rt
            .block_on(async move {
                store
                    .query_window(
                        &run_id_owned,
                        rt_min.unwrap_or(f64::NEG_INFINITY),
                        rt_max.unwrap_or(f64::INFINITY),
                        ms_level,
                    )
                    .await
            })
            .map_err(map_err)?;
        let mut spectra = batches_to_spectra(&batches);
        if let Some(n) = limit {
            spectra.truncate(n);
        }
        let list = PyList::empty_bound(py);
        for s in spectra {
            let d = PyDict::new_bound(py);
            d.set_item("run_id", s.run_id)?;
            d.set_item("scan_num", s.scan_num)?;
            d.set_item("ms_level", s.ms_level)?;
            d.set_item("rt", s.rt)?;
            d.set_item("mz", s.mz)?;
            d.set_item("intensity", s.intensity)?;
            d.set_item("tic", s.tic)?;
            let prec = s.precursor.as_ref().and_then(|p| p.mz);
            d.set_item("precursor_mz", prec)?;
            d.set_item("native_id", s.native_id)?;
            list.append(d)?;
        }
        Ok(list)
    }

    /// Fetch chromatograms for a run as a list of dicts.
    fn chromatograms<'py>(&self, py: Python<'py>, run_id: &str) -> PyResult<Bound<'py, PyList>> {
        let store = Arc::clone(&self.inner);
        let run_id_owned = run_id.to_string();
        let batches = self
            .rt
            .block_on(async move { store.read_chromatograms(&run_id_owned).await })
            .map_err(map_err)?;
        let chroms = batches_to_chromatograms(&batches);
        let list = PyList::empty_bound(py);
        for c in chroms {
            let d = PyDict::new_bound(py);
            d.set_item("run_id", c.run_id)?;
            d.set_item("chrom_id", c.chrom_id)?;
            d.set_item("chrom_type", c.chrom_type)?;
            d.set_item("precursor_mz", c.precursor_mz)?;
            d.set_item("product_mz", c.product_mz)?;
            d.set_item("time", c.time)?;
            d.set_item("intensity", c.intensity)?;
            list.append(d)?;
        }
        Ok(list)
    }
}

/// Module initialization.
#[pymodule]
fn _speclance(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<PyStore>()?;
    // Suppress unused-import warning when pyvaluerror is not invoked.
    let _ = PyValueError::new_err("");
    Ok(())
}
