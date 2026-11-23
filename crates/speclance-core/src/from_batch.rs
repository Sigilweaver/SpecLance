//! Arrow [`RecordBatch`] -> in-memory record conversions.
//!
//! These are the inverse of the builders in [`store`] and are used by
//! the export path (read tables from disk, convert to Rust types,
//! hand off to the mzML writer).
//!
//! A missing required column, or any column present with an unexpected
//! Arrow type, returns [`Error::Schema`]. Optional columns that are
//! absent read as `None`.

use arrow_array::{
    Array, BooleanArray, Float32Array, Float64Array, Int8Array, LargeListArray, RecordBatch,
    StringArray, UInt32Array, UInt8Array,
};

use crate::error::{Error, Result};
use crate::types::{Chromatogram, Precursor, Run, Spectrum};

pub fn batches_to_runs(batches: &[RecordBatch]) -> Result<Vec<Run>> {
    let mut out = Vec::new();
    for b in batches {
        let n = b.num_rows();
        let run_id = col_str(b, "run_id")?;
        let src_path = col_opt_str(b, "source_path")?;
        let src_fmt = col_str(b, "source_format")?;
        let instrument = col_opt_str(b, "instrument")?;
        let start = col_opt_str(b, "start_time")?;
        let ingested = col_opt_str(b, "ingested_at")?;
        let spec_count = col_opt_u32(b, "spectrum_count")?;
        let ms1 = col_opt_u32(b, "ms1_count")?;
        let ms2 = col_opt_u32(b, "ms2_count")?;
        let meta = col_opt_str(b, "run_metadata")?;
        for i in 0..n {
            out.push(Run {
                run_id: run_id.value(i).to_string(),
                source_path: src_path.as_ref().and_then(|a| nullable(a, i)),
                source_format: src_fmt.value(i).to_string(),
                instrument: instrument.as_ref().and_then(|a| nullable(a, i)),
                start_time: start.as_ref().and_then(|a| nullable(a, i)),
                ingested_at: ingested.as_ref().and_then(|a| nullable(a, i)),
                spectrum_count: spec_count.as_ref().and_then(|a| nullable_u32(a, i)),
                ms1_count: ms1.as_ref().and_then(|a| nullable_u32(a, i)),
                ms2_count: ms2.as_ref().and_then(|a| nullable_u32(a, i)),
                run_metadata: meta.as_ref().and_then(|a| nullable(a, i)),
            });
        }
    }
    Ok(out)
}

pub fn batches_to_spectra(batches: &[RecordBatch]) -> Result<Vec<Spectrum>> {
    let mut out = Vec::new();
    for b in batches {
        let n = b.num_rows();
        let run_id = col_str(b, "run_id")?;
        let scan_num = col_u32(b, "scan_num")?;
        let native_id = col_opt_str(b, "native_id")?;
        let ms_level = col_u8(b, "ms_level")?;
        let rt = col_opt_f64(b, "rt")?;
        let tic = col_opt_f64(b, "tic")?;
        let bp_mz = col_opt_f64(b, "base_peak_mz")?;
        let bp_int = col_opt_f64(b, "base_peak_intensity")?;
        let polarity = col_opt_i8(b, "polarity")?;
        let centroided = col_opt_bool(b, "centroided")?;
        let prec_mz = col_opt_f64(b, "precursor_mz")?;
        let prec_chg = col_opt_i8(b, "precursor_charge")?;
        let prec_int = col_opt_f64(b, "precursor_intensity")?;
        let iso_tgt = col_opt_f64(b, "isolation_window_target")?;
        let iso_lo = col_opt_f64(b, "isolation_window_lower")?;
        let iso_hi = col_opt_f64(b, "isolation_window_upper")?;
        let activation = col_opt_str(b, "activation")?;
        let ce = col_opt_f32(b, "collision_energy")?;
        let im = col_opt_f64(b, "inv_mobility")?;
        let mz_prec = col_opt_u8(b, "mz_precision")?;
        let int_prec = col_opt_u8(b, "intensity_precision")?;
        let scan_lo = col_opt_f64(b, "scan_window_lower")?;
        let scan_hi = col_opt_f64(b, "scan_window_upper")?;
        let mz = col_large_list_f64(b, "mz")?;
        let intensity = col_large_list_f32(b, "intensity")?;
        let cv = col_opt_str(b, "cv_params")?;

        for i in 0..n {
            let p_mz = prec_mz.as_ref().and_then(|a| nullable_f64(a, i));
            let p_chg = prec_chg.as_ref().and_then(|a| nullable_i8(a, i));
            let p_int = prec_int.as_ref().and_then(|a| nullable_f64(a, i));
            let p_tgt = iso_tgt.as_ref().and_then(|a| nullable_f64(a, i));
            let p_lo = iso_lo.as_ref().and_then(|a| nullable_f64(a, i));
            let p_hi = iso_hi.as_ref().and_then(|a| nullable_f64(a, i));
            let precursor = if p_mz.is_some()
                || p_chg.is_some()
                || p_int.is_some()
                || p_tgt.is_some()
                || p_lo.is_some()
                || p_hi.is_some()
            {
                Some(Precursor {
                    mz: p_mz,
                    charge: p_chg,
                    intensity: p_int,
                    isolation_window_target: p_tgt,
                    isolation_window_lower: p_lo,
                    isolation_window_upper: p_hi,
                })
            } else {
                None
            };

            out.push(Spectrum {
                run_id: run_id.value(i).to_string(),
                scan_num: scan_num.value(i),
                native_id: native_id.as_ref().and_then(|a| nullable(a, i)),
                ms_level: ms_level.value(i),
                rt: rt.as_ref().and_then(|a| nullable_f64(a, i)),
                tic: tic.as_ref().and_then(|a| nullable_f64(a, i)),
                base_peak_mz: bp_mz.as_ref().and_then(|a| nullable_f64(a, i)),
                base_peak_intensity: bp_int.as_ref().and_then(|a| nullable_f64(a, i)),
                polarity: polarity.as_ref().and_then(|a| nullable_i8(a, i)),
                centroided: centroided.as_ref().and_then(|a| nullable_bool(a, i)),
                precursor,
                activation: activation.as_ref().and_then(|a| nullable(a, i)),
                collision_energy: ce.as_ref().and_then(|a| nullable_f32(a, i)),
                inv_mobility: im.as_ref().and_then(|a| nullable_f64(a, i)),
                mz_precision: mz_prec.as_ref().and_then(|a| nullable_u8(a, i)),
                intensity_precision: int_prec.as_ref().and_then(|a| nullable_u8(a, i)),
                scan_window_lower: scan_lo.as_ref().and_then(|a| nullable_f64(a, i)),
                scan_window_upper: scan_hi.as_ref().and_then(|a| nullable_f64(a, i)),
                mz: list_row_f64(&mz, i)?,
                intensity: list_row_f32(&intensity, i)?,
                cv_params: cv.as_ref().and_then(|a| nullable(a, i)),
            });
        }
    }
    // Sort by scan_num so the writer emits in source order.
    out.sort_by_key(|s| s.scan_num);
    Ok(out)
}

pub fn batches_to_chromatograms(batches: &[RecordBatch]) -> Result<Vec<Chromatogram>> {
    let mut out = Vec::new();
    for b in batches {
        let n = b.num_rows();
        let run_id = col_str(b, "run_id")?;
        let cid = col_str(b, "chrom_id")?;
        let ctype = col_opt_str(b, "chrom_type")?;
        let pmz = col_opt_f64(b, "precursor_mz")?;
        let qmz = col_opt_f64(b, "product_mz")?;
        let time = col_large_list_f32(b, "time")?;
        let intensity = col_large_list_f32(b, "intensity")?;
        let cv = col_opt_str(b, "cv_params")?;
        for i in 0..n {
            out.push(Chromatogram {
                run_id: run_id.value(i).to_string(),
                chrom_id: cid.value(i).to_string(),
                chrom_type: ctype.as_ref().and_then(|a| nullable(a, i)),
                precursor_mz: pmz.as_ref().and_then(|a| nullable_f64(a, i)),
                product_mz: qmz.as_ref().and_then(|a| nullable_f64(a, i)),
                time: list_row_f32(&time, i)?,
                intensity: list_row_f32(&intensity, i)?,
                cv_params: cv.as_ref().and_then(|a| nullable(a, i)),
            });
        }
    }
    Ok(out)
}

// -- column accessors --

fn type_err(b: &RecordBatch, name: &str, want: &str) -> Error {
    let got = b
        .schema()
        .field_with_name(name)
        .map(|f| f.data_type().to_string())
        .unwrap_or_else(|_| "?".into());
    Error::Schema(format!("column {name:?}: expected {want}, found {got}"))
}

fn col<'a, T: 'static>(b: &'a RecordBatch, name: &str, want: &str) -> Result<&'a T> {
    let idx = b
        .schema()
        .index_of(name)
        .map_err(|_| Error::Schema(format!("missing required column {name:?}")))?;
    b.column(idx)
        .as_any()
        .downcast_ref::<T>()
        .ok_or_else(|| type_err(b, name, want))
}
fn col_opt<'a, T: 'static>(b: &'a RecordBatch, name: &str, want: &str) -> Result<Option<&'a T>> {
    let Ok(idx) = b.schema().index_of(name) else {
        return Ok(None);
    };
    b.column(idx)
        .as_any()
        .downcast_ref::<T>()
        .map(Some)
        .ok_or_else(|| type_err(b, name, want))
}

fn col_str<'a>(b: &'a RecordBatch, name: &str) -> Result<&'a StringArray> {
    col::<StringArray>(b, name, "Utf8")
}
fn col_opt_str<'a>(b: &'a RecordBatch, name: &str) -> Result<Option<&'a StringArray>> {
    col_opt::<StringArray>(b, name, "Utf8")
}
fn col_u32<'a>(b: &'a RecordBatch, name: &str) -> Result<&'a UInt32Array> {
    col::<UInt32Array>(b, name, "UInt32")
}
fn col_opt_u32<'a>(b: &'a RecordBatch, name: &str) -> Result<Option<&'a UInt32Array>> {
    col_opt::<UInt32Array>(b, name, "UInt32")
}
fn col_u8<'a>(b: &'a RecordBatch, name: &str) -> Result<&'a UInt8Array> {
    col::<UInt8Array>(b, name, "UInt8")
}
fn col_opt_u8<'a>(b: &'a RecordBatch, name: &str) -> Result<Option<&'a UInt8Array>> {
    col_opt::<UInt8Array>(b, name, "UInt8")
}
fn col_opt_i8<'a>(b: &'a RecordBatch, name: &str) -> Result<Option<&'a Int8Array>> {
    col_opt::<Int8Array>(b, name, "Int8")
}
fn col_opt_f32<'a>(b: &'a RecordBatch, name: &str) -> Result<Option<&'a Float32Array>> {
    col_opt::<Float32Array>(b, name, "Float32")
}
fn col_opt_f64<'a>(b: &'a RecordBatch, name: &str) -> Result<Option<&'a Float64Array>> {
    col_opt::<Float64Array>(b, name, "Float64")
}
fn col_opt_bool<'a>(b: &'a RecordBatch, name: &str) -> Result<Option<&'a BooleanArray>> {
    col_opt::<BooleanArray>(b, name, "Boolean")
}
fn col_large_list(b: &RecordBatch, name: &str) -> Result<LargeListArray> {
    Ok(col::<LargeListArray>(b, name, "LargeList")?.clone())
}
fn col_large_list_f64(b: &RecordBatch, name: &str) -> Result<LargeListArray> {
    col_large_list(b, name)
}
fn col_large_list_f32(b: &RecordBatch, name: &str) -> Result<LargeListArray> {
    col_large_list(b, name)
}

fn nullable(a: &StringArray, i: usize) -> Option<String> {
    if a.is_null(i) {
        None
    } else {
        Some(a.value(i).to_string())
    }
}
fn nullable_u32(a: &UInt32Array, i: usize) -> Option<u32> {
    if a.is_null(i) {
        None
    } else {
        Some(a.value(i))
    }
}
fn nullable_u8(a: &UInt8Array, i: usize) -> Option<u8> {
    if a.is_null(i) {
        None
    } else {
        Some(a.value(i))
    }
}
fn nullable_i8(a: &Int8Array, i: usize) -> Option<i8> {
    if a.is_null(i) {
        None
    } else {
        Some(a.value(i))
    }
}
fn nullable_f32(a: &Float32Array, i: usize) -> Option<f32> {
    if a.is_null(i) {
        None
    } else {
        Some(a.value(i))
    }
}
fn nullable_f64(a: &Float64Array, i: usize) -> Option<f64> {
    if a.is_null(i) {
        None
    } else {
        Some(a.value(i))
    }
}
fn nullable_bool(a: &BooleanArray, i: usize) -> Option<bool> {
    if a.is_null(i) {
        None
    } else {
        Some(a.value(i))
    }
}

fn list_row_f64(arr: &LargeListArray, i: usize) -> Result<Vec<f64>> {
    let v = arr.value(i);
    let f = v.as_any().downcast_ref::<Float64Array>().ok_or_else(|| {
        Error::Schema(format!(
            "list column: expected Float64 items, found {}",
            v.data_type()
        ))
    })?;
    Ok(f.values().to_vec())
}
fn list_row_f32(arr: &LargeListArray, i: usize) -> Result<Vec<f32>> {
    let v = arr.value(i);
    let f = v.as_any().downcast_ref::<Float32Array>().ok_or_else(|| {
        Error::Schema(format!(
            "list column: expected Float32 items, found {}",
            v.data_type()
        ))
    })?;
    Ok(f.values().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use arrow_array::ArrayRef;
    use arrow_schema::{DataType, Field, Schema};

    #[test]
    fn missing_required_column_is_schema_error() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "source_format",
            DataType::Utf8,
            false,
        )]));
        let col: ArrayRef = Arc::new(StringArray::from(vec!["mzml"]));
        let b = RecordBatch::try_new(schema, vec![col]).unwrap();
        let err = batches_to_runs(&[b]).unwrap_err();
        assert!(
            matches!(err, Error::Schema(ref m) if m.contains("run_id")),
            "{err}"
        );
    }

    #[test]
    fn mistyped_column_is_schema_error() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("run_id", DataType::UInt32, false),
            Field::new("source_format", DataType::Utf8, false),
        ]));
        let cols: Vec<ArrayRef> = vec![
            Arc::new(UInt32Array::from(vec![1u32])),
            Arc::new(StringArray::from(vec!["mzml"])),
        ];
        let b = RecordBatch::try_new(schema, cols).unwrap();
        let err = batches_to_runs(&[b]).unwrap_err();
        assert!(
            matches!(err, Error::Schema(ref m) if m.contains("Utf8")),
            "{err}"
        );
    }

    #[test]
    fn mistyped_optional_column_is_schema_error() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("run_id", DataType::Utf8, false),
            Field::new("source_format", DataType::Utf8, false),
            Field::new("spectrum_count", DataType::Utf8, true),
        ]));
        let cols: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from(vec!["r1"])),
            Arc::new(StringArray::from(vec!["mzml"])),
            Arc::new(StringArray::from(vec!["7"])),
        ];
        let b = RecordBatch::try_new(schema, cols).unwrap();
        assert!(matches!(batches_to_runs(&[b]), Err(Error::Schema(_))));
    }
}
