use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use prolance_core::{batches_to_chromatograms, batches_to_runs, batches_to_spectra, Store};
use prolance_ms::mzml::{read_mzml, write_mzml};

#[derive(Parser)]
#[command(name = "prolance", version, about = "Columnar mass spectrometry storage")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Ingest one or more files into a ProLance store.
    Ingest {
        /// Path to the ProLance store directory.
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Ingest { store, inputs } => cmd_ingest(&store, &inputs).await,
        Cmd::Runs { store } => cmd_runs(&store).await,
        Cmd::Export { store, run_id, out } => cmd_export(&store, &run_id, &out).await,
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
    let data = match ext.as_str() {
        "mzml" => read_mzml(path).context("read mzml")?,
        "raw" if path.is_file() => {
            prolance_ms::thermo::ingest(path).context("read thermo raw")?
        }
        "raw" if path.is_dir() => {
            prolance_ms::waters::ingest(path).context("read waters raw dir")?
        }
        "d" if path.is_dir() => {
            prolance_ms::bruker::ingest(path).context("read bruker .d")?
        }
        other => anyhow::bail!("unsupported extension/kind: .{}", other),
    };
    eprintln!(
        "  parsed {} spectra, {} chromatograms (run_id={})",
        data.spectra.len(),
        data.chromatograms.len(),
        data.run.run_id
    );
    store.append_run(&data.run).await?;
    for chunk in data.spectra.chunks(2048) {
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
