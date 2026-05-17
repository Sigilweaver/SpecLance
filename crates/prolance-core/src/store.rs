//! Lance store handle.
//!
//! Layout on disk:
//! ```text
//! <store>/
//!   runs.lance/           - run registry
//!   spectra.lance/        - spectra (peak arrays as list columns)
//!   chromatograms.lance/  - chromatograms (TIC, BPC, SRM)
//! ```

use std::sync::Arc;

use arrow_array::{
    builder::{Float32Builder, Float64Builder, LargeListBuilder},
    ArrayRef, BooleanArray, Float32Array, Float64Array, Int8Array, RecordBatch, StringArray,
    UInt32Array, UInt8Array,
};
use futures::TryStreamExt;
use lancedb::{
    query::{ExecutableQuery, QueryBase},
    Connection, Table,
};

use crate::error::Result;
use crate::schema::{
    chromatograms_schema, runs_schema, spectra_schema, CHROMATOGRAMS_TABLE, RUNS_TABLE,
    SPECTRA_TABLE,
};
use crate::types::{Chromatogram, Run, Spectrum};

/// Handle to an open ProLance store.
pub struct Store {
    pub conn: Connection,
}

impl Store {
    /// Open (or create) a ProLance store at `root`.
    pub async fn open(root: &str) -> Result<Self> {
        std::fs::create_dir_all(root)?;
        let conn = lancedb::connect(root).execute().await?;
        Ok(Self { conn })
    }

    /// List all tables present in this store.
    pub async fn table_names(&self) -> Result<Vec<String>> {
        Ok(self.conn.table_names().execute().await?)
    }

    /// Open or create the runs table.
    pub async fn runs_table(&self) -> Result<Table> {
        open_or_create(&self.conn, RUNS_TABLE, runs_schema()).await
    }

    /// Open or create the spectra table.
    pub async fn spectra_table(&self) -> Result<Table> {
        open_or_create(&self.conn, SPECTRA_TABLE, spectra_schema()).await
    }

    /// Open or create the chromatograms table.
    pub async fn chromatograms_table(&self) -> Result<Table> {
        open_or_create(&self.conn, CHROMATOGRAMS_TABLE, chromatograms_schema()).await
    }

    /// Append a single run row.
    pub async fn append_run(&self, run: &Run) -> Result<()> {
        let batch = run_to_batch(std::slice::from_ref(run))?;
        let table = self.runs_table().await?;
        table.add(vec![batch]).execute().await?;
        Ok(())
    }

    /// Append a batch of spectra rows.
    pub async fn append_spectra(&self, spectra: &[Spectrum]) -> Result<()> {
        if spectra.is_empty() {
            return Ok(());
        }
        let batch = spectra_to_batch(spectra)?;
        let table = self.spectra_table().await?;
        table.add(vec![batch]).execute().await?;
        Ok(())
    }

    /// Append a batch of chromatograms.
    pub async fn append_chromatograms(&self, chroms: &[Chromatogram]) -> Result<()> {
        if chroms.is_empty() {
            return Ok(());
        }
        let batch = chromatograms_to_batch(chroms)?;
        let table = self.chromatograms_table().await?;
        table.add(vec![batch]).execute().await?;
        Ok(())
    }

    /// Read all spectra for a given run, ordered by scan_num.
    pub async fn read_spectra(&self, run_id: &str) -> Result<Vec<RecordBatch>> {
        let table = self.spectra_table().await?;
        let filter = format!("run_id = '{}'", run_id.replace('\'', "''"));
        let stream = table.query().only_if(filter).execute().await?;
        let batches: Vec<RecordBatch> = stream.try_collect().await?;
        Ok(batches)
    }

    /// Read a single run row by id, if present.
    pub async fn read_run(&self, run_id: &str) -> Result<Option<RecordBatch>> {
        let table = self.runs_table().await?;
        let filter = format!("run_id = '{}'", run_id.replace('\'', "''"));
        let stream = table.query().only_if(filter).execute().await?;
        let batches: Vec<RecordBatch> = stream.try_collect().await?;
        Ok(batches.into_iter().next())
    }

    /// Read all chromatograms for a given run.
    pub async fn read_chromatograms(&self, run_id: &str) -> Result<Vec<RecordBatch>> {
        let names = self.table_names().await?;
        if !names.iter().any(|n| n == CHROMATOGRAMS_TABLE) {
            return Ok(Vec::new());
        }
        let table = self.chromatograms_table().await?;
        let filter = format!("run_id = '{}'", run_id.replace('\'', "''"));
        let stream = table.query().only_if(filter).execute().await?;
        let batches: Vec<RecordBatch> = stream.try_collect().await?;
        Ok(batches)
    }
}

async fn open_or_create(
    conn: &Connection,
    name: &str,
    schema: Arc<arrow_schema::Schema>,
) -> Result<Table> {
    let names = conn.table_names().execute().await?;
    if names.iter().any(|n| n == name) {
        Ok(conn.open_table(name).execute().await?)
    } else {
        Ok(conn.create_empty_table(name, schema).execute().await?)
    }
}

// ── Arrow conversion helpers ─────────────────────────────────────────────────

fn run_to_batch(runs: &[Run]) -> Result<RecordBatch> {
    let schema = runs_schema();
    let run_id: ArrayRef = Arc::new(StringArray::from_iter_values(runs.iter().map(|r| &r.run_id)));
    let src_path: ArrayRef = Arc::new(StringArray::from(
        runs.iter()
            .map(|r| r.source_path.clone())
            .collect::<Vec<_>>(),
    ));
    let src_fmt: ArrayRef = Arc::new(StringArray::from_iter_values(
        runs.iter().map(|r| &r.source_format),
    ));
    let instrument: ArrayRef = Arc::new(StringArray::from(
        runs.iter().map(|r| r.instrument.clone()).collect::<Vec<_>>(),
    ));
    let start_time: ArrayRef = Arc::new(StringArray::from(
        runs.iter().map(|r| r.start_time.clone()).collect::<Vec<_>>(),
    ));
    let ingested_at: ArrayRef = Arc::new(StringArray::from(
        runs.iter()
            .map(|r| r.ingested_at.clone())
            .collect::<Vec<_>>(),
    ));
    let spec_count: ArrayRef = Arc::new(UInt32Array::from(
        runs.iter().map(|r| r.spectrum_count).collect::<Vec<_>>(),
    ));
    let ms1: ArrayRef = Arc::new(UInt32Array::from(
        runs.iter().map(|r| r.ms1_count).collect::<Vec<_>>(),
    ));
    let ms2: ArrayRef = Arc::new(UInt32Array::from(
        runs.iter().map(|r| r.ms2_count).collect::<Vec<_>>(),
    ));
    let meta: ArrayRef = Arc::new(StringArray::from(
        runs.iter()
            .map(|r| r.run_metadata.clone())
            .collect::<Vec<_>>(),
    ));
    Ok(RecordBatch::try_new(
        schema,
        vec![
            run_id,
            src_path,
            src_fmt,
            instrument,
            start_time,
            ingested_at,
            spec_count,
            ms1,
            ms2,
            meta,
        ],
    )?)
}

fn spectra_to_batch(spectra: &[Spectrum]) -> Result<RecordBatch> {
    let schema = spectra_schema();

    let run_id: ArrayRef =
        Arc::new(StringArray::from_iter_values(spectra.iter().map(|s| &s.run_id)));
    let scan_num: ArrayRef =
        Arc::new(UInt32Array::from_iter_values(spectra.iter().map(|s| s.scan_num)));
    let native_id: ArrayRef = Arc::new(StringArray::from(
        spectra.iter().map(|s| s.native_id.clone()).collect::<Vec<_>>(),
    ));
    let ms_level: ArrayRef =
        Arc::new(UInt8Array::from_iter_values(spectra.iter().map(|s| s.ms_level)));
    let rt: ArrayRef = Arc::new(Float64Array::from(
        spectra.iter().map(|s| s.rt).collect::<Vec<_>>(),
    ));
    let tic: ArrayRef = Arc::new(Float64Array::from(
        spectra.iter().map(|s| s.tic).collect::<Vec<_>>(),
    ));
    let bp_mz: ArrayRef = Arc::new(Float64Array::from(
        spectra.iter().map(|s| s.base_peak_mz).collect::<Vec<_>>(),
    ));
    let bp_int: ArrayRef = Arc::new(Float64Array::from(
        spectra
            .iter()
            .map(|s| s.base_peak_intensity)
            .collect::<Vec<_>>(),
    ));
    let polarity: ArrayRef = Arc::new(Int8Array::from(
        spectra.iter().map(|s| s.polarity).collect::<Vec<_>>(),
    ));
    let centroided: ArrayRef = Arc::new(BooleanArray::from(
        spectra.iter().map(|s| s.centroided).collect::<Vec<_>>(),
    ));
    let prec_mz: ArrayRef = Arc::new(Float64Array::from(
        spectra
            .iter()
            .map(|s| s.precursor.as_ref().and_then(|p| p.mz))
            .collect::<Vec<_>>(),
    ));
    let prec_charge: ArrayRef = Arc::new(Int8Array::from(
        spectra
            .iter()
            .map(|s| s.precursor.as_ref().and_then(|p| p.charge))
            .collect::<Vec<_>>(),
    ));
    let prec_int: ArrayRef = Arc::new(Float64Array::from(
        spectra
            .iter()
            .map(|s| s.precursor.as_ref().and_then(|p| p.intensity))
            .collect::<Vec<_>>(),
    ));
    let iso_tgt: ArrayRef = Arc::new(Float64Array::from(
        spectra
            .iter()
            .map(|s| s.precursor.as_ref().and_then(|p| p.isolation_window_target))
            .collect::<Vec<_>>(),
    ));
    let iso_lo: ArrayRef = Arc::new(Float64Array::from(
        spectra
            .iter()
            .map(|s| s.precursor.as_ref().and_then(|p| p.isolation_window_lower))
            .collect::<Vec<_>>(),
    ));
    let iso_hi: ArrayRef = Arc::new(Float64Array::from(
        spectra
            .iter()
            .map(|s| s.precursor.as_ref().and_then(|p| p.isolation_window_upper))
            .collect::<Vec<_>>(),
    ));
    let activation: ArrayRef = Arc::new(StringArray::from(
        spectra.iter().map(|s| s.activation.clone()).collect::<Vec<_>>(),
    ));
    let ce: ArrayRef = Arc::new(Float32Array::from(
        spectra
            .iter()
            .map(|s| s.collision_energy)
            .collect::<Vec<_>>(),
    ));
    let im: ArrayRef = Arc::new(Float64Array::from(
        spectra.iter().map(|s| s.inv_mobility).collect::<Vec<_>>(),
    ));
    let mz_prec: ArrayRef = Arc::new(UInt8Array::from(
        spectra.iter().map(|s| s.mz_precision).collect::<Vec<_>>(),
    ));
    let int_prec: ArrayRef = Arc::new(UInt8Array::from(
        spectra
            .iter()
            .map(|s| s.intensity_precision)
            .collect::<Vec<_>>(),
    ));
    let scan_lo: ArrayRef = Arc::new(Float64Array::from(
        spectra.iter().map(|s| s.scan_window_lower).collect::<Vec<_>>(),
    ));
    let scan_hi: ArrayRef = Arc::new(Float64Array::from(
        spectra.iter().map(|s| s.scan_window_upper).collect::<Vec<_>>(),
    ));

    // LargeList<Float64> for m/z
    let mut mz_builder =
        LargeListBuilder::new(Float64Builder::new()).with_field(Arc::new(arrow_schema::Field::new(
            "item",
            arrow_schema::DataType::Float64,
            false,
        )));
    for s in spectra {
        for &v in &s.mz {
            mz_builder.values().append_value(v);
        }
        mz_builder.append(true);
    }
    let mz: ArrayRef = Arc::new(mz_builder.finish());

    // LargeList<Float32> for intensity
    let mut int_builder =
        LargeListBuilder::new(Float32Builder::new()).with_field(Arc::new(arrow_schema::Field::new(
            "item",
            arrow_schema::DataType::Float32,
            false,
        )));
    for s in spectra {
        for &v in &s.intensity {
            int_builder.values().append_value(v);
        }
        int_builder.append(true);
    }
    let intensity: ArrayRef = Arc::new(int_builder.finish());

    let cv: ArrayRef = Arc::new(StringArray::from(
        spectra.iter().map(|s| s.cv_params.clone()).collect::<Vec<_>>(),
    ));

    Ok(RecordBatch::try_new(
        schema,
        vec![
            run_id, scan_num, native_id, ms_level, rt, tic, bp_mz, bp_int, polarity, centroided,
            prec_mz, prec_charge, prec_int, iso_tgt, iso_lo, iso_hi, activation, ce, im, mz_prec,
            int_prec, scan_lo, scan_hi, mz, intensity, cv,
        ],
    )?)
}

fn chromatograms_to_batch(chroms: &[Chromatogram]) -> Result<RecordBatch> {
    let schema = chromatograms_schema();
    let run_id: ArrayRef = Arc::new(StringArray::from_iter_values(chroms.iter().map(|c| &c.run_id)));
    let cid: ArrayRef = Arc::new(StringArray::from_iter_values(chroms.iter().map(|c| &c.chrom_id)));
    let ctype: ArrayRef = Arc::new(StringArray::from(
        chroms.iter().map(|c| c.chrom_type.clone()).collect::<Vec<_>>(),
    ));
    let pmz: ArrayRef = Arc::new(Float64Array::from(
        chroms.iter().map(|c| c.precursor_mz).collect::<Vec<_>>(),
    ));
    let qmz: ArrayRef = Arc::new(Float64Array::from(
        chroms.iter().map(|c| c.product_mz).collect::<Vec<_>>(),
    ));

    let mut t_builder = LargeListBuilder::new(Float32Builder::new()).with_field(Arc::new(
        arrow_schema::Field::new("item", arrow_schema::DataType::Float32, false),
    ));
    for c in chroms {
        for &v in &c.time {
            t_builder.values().append_value(v);
        }
        t_builder.append(true);
    }
    let time: ArrayRef = Arc::new(t_builder.finish());

    let mut i_builder = LargeListBuilder::new(Float32Builder::new()).with_field(Arc::new(
        arrow_schema::Field::new("item", arrow_schema::DataType::Float32, false),
    ));
    for c in chroms {
        for &v in &c.intensity {
            i_builder.values().append_value(v);
        }
        i_builder.append(true);
    }
    let intensity: ArrayRef = Arc::new(i_builder.finish());

    let cv: ArrayRef = Arc::new(StringArray::from(
        chroms.iter().map(|c| c.cv_params.clone()).collect::<Vec<_>>(),
    ));

    Ok(RecordBatch::try_new(
        schema,
        vec![run_id, cid, ctype, pmz, qmz, time, intensity, cv],
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Precursor;

    #[tokio::test]
    async fn create_and_append() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().to_str().unwrap();
        let store = Store::open(path).await.unwrap();

        let run = Run {
            run_id: "abc123".into(),
            source_path: Some("/tmp/x.mzML".into()),
            source_format: "mzml".into(),
            instrument: Some("Orbitrap".into()),
            spectrum_count: Some(1),
            ms1_count: Some(1),
            ms2_count: Some(0),
            ..Default::default()
        };
        store.append_run(&run).await.unwrap();

        let spec = Spectrum {
            run_id: "abc123".into(),
            scan_num: 1,
            ms_level: 1,
            rt: Some(0.5),
            mz: vec![100.0, 200.0, 300.0],
            intensity: vec![1.0, 2.0, 3.0],
            mz_precision: Some(64),
            intensity_precision: Some(32),
            precursor: Some(Precursor::default()),
            ..Default::default()
        };
        store.append_spectra(&[spec]).await.unwrap();

        let names = store.table_names().await.unwrap();
        assert!(names.iter().any(|n| n == "runs"));
        assert!(names.iter().any(|n| n == "spectra"));

        let read = store.read_run("abc123").await.unwrap().unwrap();
        assert_eq!(read.num_rows(), 1);

        let read_spectra = store.read_spectra("abc123").await.unwrap();
        let total: usize = read_spectra.iter().map(|b| b.num_rows()).sum();
        assert_eq!(total, 1);
    }
}
