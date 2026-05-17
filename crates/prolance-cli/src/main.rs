use anyhow::Result;
use clap::{Parser, Subcommand};

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
        /// Input file(s): .mzML, .raw (Thermo/Waters), or .d/ (Bruker).
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
    let s = prolance_core::Store::open(store).await?;
    for path in inputs {
        eprintln!("ingest: {}", path);
        let _ = (&s, path);
        // TODO: dispatch based on extension; implemented in later commits.
        anyhow::bail!("ingest not yet implemented");
    }
    Ok(())
}

async fn cmd_runs(store: &str) -> Result<()> {
    let s = prolance_core::Store::open(store).await?;
    let names = s.table_names().await?;
    println!("tables: {:?}", names);
    Ok(())
}

async fn cmd_export(_store: &str, _run_id: &str, _out: &str) -> Result<()> {
    anyhow::bail!("export not yet implemented");
}
