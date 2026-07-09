use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use speclance_core::{batches_to_chromatograms, batches_to_runs, batches_to_spectra, Store};
use speclance_ms::mzml::{write_mzml, MzmlIngest};

#[derive(Parser)]
#[command(
    name = "speclance",
    version,
    about = "Columnar mass spectrometry storage"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Ingest one or more files into a SpecLance store.
    Ingest {
        /// Path to the SpecLance store directory.
        #[arg(long)]
        store: String,
        /// Input file(s): .mzML (Thermo/Bruker/Waters vendor support
        /// added in later versions).
        #[arg(required = true)]
        inputs: Vec<String>,
    },
    /// List runs in a store.
    Runs {
        #[arg(long)]
        store: String,
    },
    /// Export a run back to mzML.
    Export {
        #[arg(long)]
        store: String,
        #[arg(long)]
        run_id: String,
        #[arg(long)]
        out: String,
    },
    /// Build the default scalar indexes on the spectra and runs
    /// tables (BTree on run_id, scan_num, rt; Bitmap on ms_level).
    Index {
        #[arg(long)]
        store: String,
    },
    /// Range-query spectra in a retention-time window for a run.
    /// Prints one summary line per spectrum (scan_num, ms_level,
    /// rt, peak count, tic).
    Query {
        #[arg(long)]
        store: String,
        #[arg(long)]
        run_id: String,
        /// Retention-time lower bound in seconds (inclusive).
        #[arg(long)]
        rt_min: Option<f64>,
        /// Retention-time upper bound in seconds (inclusive).
        #[arg(long)]
        rt_max: Option<f64>,
        /// Restrict to one MS level (e.g. --ms-level 1).
        #[arg(long)]
        ms_level: Option<u8>,
        /// Maximum rows to print.
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Ingest { store, inputs } => cmd_ingest(&store, &inputs).await,
        Cmd::Runs { store } => cmd_runs(&store).await,
        Cmd::Export { store, run_id, out } => cmd_export(&store, &run_id, &out).await,
        Cmd::Index { store } => cmd_index(&store).await,
        Cmd::Query {
            store,
            run_id,
            rt_min,
            rt_max,
            ms_level,
            limit,
        } => cmd_query(&store, &run_id, rt_min, rt_max, ms_level, limit).await,
    }
}

async fn cmd_ingest(store: &str, inputs: &[String]) -> Result<()> {
    let s = Store::open(store).await?;
    for path in inputs {
        eprintln!("ingest: {}", path);
        ingest_one(&s, Path::new(path)).await?;
    }
    Ok(())
}

async fn ingest_one(store: &Store, path: &Path) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "mzml" => ingest_mzml_streaming(store, path).await,
        #[cfg(feature = "vendors")]
        _ if path.is_file() || path.is_dir() => {
            let data = speclance_ms::vendor::ingest(path).context("read vendor bundle")?;
            ingest_buffered(store, data).await
        }
        other => anyhow::bail!(
            "unsupported extension/kind: .{} (build with --features all-vendors to enable vendor adapters)",
            other
        ),
    }
}

const SPECTRUM_CHUNK: usize = 2048;

/// Stream spectra straight from the mzML reader into the store in
/// `SPECTRUM_CHUNK`-sized batches. Memory usage is bounded by the
/// source byte buffer plus one chunk of decoded peak arrays, which
/// keeps multi-GB mzML ingests within reasonable RSS.
async fn ingest_mzml_streaming(store: &Store, path: &Path) -> Result<()> {
    let mut ingest = MzmlIngest::open(path).context("open mzml")?;
    let run_id = ingest.run_id().to_string();
    let mut buf: Vec<speclance_core::Spectrum> = Vec::with_capacity(SPECTRUM_CHUNK);
    let mut total = 0u32;
    let mut ms1 = 0u32;
    let mut ms2 = 0u32;
    while let Some(s) = ingest.next_spectrum().context("parse spectrum")? {
        if s.ms_level == 1 {
            ms1 += 1;
        } else if s.ms_level >= 2 {
            ms2 += 1;
        }
        total += 1;
        buf.push(s);
        if buf.len() >= SPECTRUM_CHUNK {
            store.append_spectra(&buf).await?;
            buf.clear();
        }
    }
    if !buf.is_empty() {
        store.append_spectra(&buf).await?;
        buf.clear();
    }
    let chromatograms = ingest.read_chromatograms().context("parse chromatograms")?;
    let run = ingest
        .finalize_run(total, ms1, ms2)
        .context("finalize run")?;
    eprintln!(
        "  parsed {} spectra, {} chromatograms (run_id={})",
        total,
        chromatograms.len(),
        run_id
    );
    store.append_run(&run).await?;
    if !chromatograms.is_empty() {
        store.append_chromatograms(&chromatograms).await?;
    }
    Ok(())
}

/// Buffered ingest path for vendor adapters that return a complete
/// [`MzmlData`] (`thermo`, `bruker`, `waters`). Vendor sources are
/// converted to mzML in-memory so the working set is already bounded
/// by the converter's own buffering.
#[cfg(feature = "vendors")]
async fn ingest_buffered(store: &Store, data: speclance_ms::mzml::MzmlData) -> Result<()> {
    eprintln!(
        "  parsed {} spectra, {} chromatograms (run_id={})",
        data.spectra.len(),
        data.chromatograms.len(),
        data.run.run_id
    );
    store.append_run(&data.run).await?;
    for chunk in data.spectra.chunks(SPECTRUM_CHUNK) {
        store.append_spectra(chunk).await?;
    }
    if !data.chromatograms.is_empty() {
        store.append_chromatograms(&data.chromatograms).await?;
    }
    Ok(())
}

async fn cmd_runs(store: &str) -> Result<()> {
    let s = Store::open(store).await?;
    let names = s.table_names().await?;
    if !names.iter().any(|n| n == "runs") {
        println!("(no runs)");
        return Ok(());
    }
    use futures::TryStreamExt;
    use lancedb::query::ExecutableQuery;
    let table = s.runs_table().await?;
    let stream = table.query().execute().await?;
    let batches: Vec<_> = stream.try_collect().await?;
    let runs = batches_to_runs(&batches);
    println!(
        "{:<20} {:<10} {:<25} {:<15}",
        "run_id", "format", "instrument", "spectra"
    );
    for r in &runs {
        println!(
            "{:<20} {:<10} {:<25} {:<15}",
            r.run_id,
            r.source_format,
            r.instrument.clone().unwrap_or_default(),
            r.spectrum_count.unwrap_or(0)
        );
    }
    Ok(())
}

async fn cmd_export(store: &str, run_id: &str, out: &str) -> Result<()> {
    let s = Store::open(store).await?;
    let run_batch = s
        .read_run(run_id)
        .await?
        .context("run not found in store")?;
    let runs = batches_to_runs(std::slice::from_ref(&run_batch));
    let run = runs.into_iter().next().context("empty run batch")?;
    let spec_batches = s.read_spectra(run_id).await?;
    let spectra = batches_to_spectra(&spec_batches);
    let chrom_batches = s.read_chromatograms(run_id).await?;
    let chromatograms = batches_to_chromatograms(&chrom_batches);

    let path = PathBuf::from(out);
    let mut file = std::fs::File::create(&path)?;
    write_mzml(&mut file, &run, &spectra, &chromatograms)?;
    eprintln!(
        "wrote {} spectra and {} chromatograms -> {}",
        spectra.len(),
        chromatograms.len(),
        path.display()
    );
    Ok(())
}

async fn cmd_index(store: &str) -> Result<()> {
    let s = Store::open(store).await?;
    s.create_default_indexes()
        .await
        .context("create default indexes")?;
    eprintln!("created default scalar indexes on spectra and runs tables");
    Ok(())
}

async fn cmd_query(
    store: &str,
    run_id: &str,
    rt_min: Option<f64>,
    rt_max: Option<f64>,
    ms_level: Option<u8>,
    limit: usize,
) -> Result<()> {
    let s = Store::open(store).await?;
    let batches = s
        .query_window(
            run_id,
            rt_min.unwrap_or(f64::NEG_INFINITY),
            rt_max.unwrap_or(f64::INFINITY),
            ms_level,
        )
        .await?;
    let spectra = batches_to_spectra(&batches);
    let total = spectra.len();
    eprintln!(
        "matched {} spectra (run={}, rt=[{:?},{:?}], ms_level={:?})",
        total, run_id, rt_min, rt_max, ms_level
    );
    println!(
        "{:>8} {:>9} {:>12} {:>10} {:>14}",
        "scan", "ms_level", "rt_sec", "n_peaks", "tic"
    );
    for s in spectra.iter().take(limit) {
        println!(
            "{:>8} {:>9} {:>12.3} {:>10} {:>14.3e}",
            s.scan_num,
            s.ms_level,
            s.rt.unwrap_or(0.0),
            s.mz.len(),
            s.tic.unwrap_or(0.0)
        );
    }
    if total > limit {
        eprintln!(
            "(showing first {} of {}; use --limit to expand)",
            limit, total
        );
    }
    Ok(())
}
