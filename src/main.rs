use clap::Parser;
use siga::{server, spec::Spec};
use std::path::PathBuf;

/// Emulate a REST API from an OpenAPI 3 spec.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Path to the OpenAPI spec file (.yaml, .yml or .json).
    #[arg(long)]
    api: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    server::serve(Spec::load(&cli.api)?).await
}
