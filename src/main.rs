use anyhow::Result;
use clap::{Parser, Subcommand};
use bing::download::{run_download, Args as DownloadArgs};
use bing::decompress::{run_decompress, Args as DecompressArgs};

#[derive(Parser)]
#[command(name = "bing")]
#[command(about = "A CLI tool for downloading and processing Bing Maps tiles")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Download Bing 3D 'mtx' GLB tiles for a lat/lon rectangle
    Download(DownloadArgs),
    /// Parallel KTX2 texture decompression for .glb files using gltf-transform ktxdecompress
    Decompress(DecompressArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Download(args) => {
            run_download(args).await?;
        }
        Commands::Decompress(args) => {
            run_decompress(args)?;
        }
    }

    Ok(())
}