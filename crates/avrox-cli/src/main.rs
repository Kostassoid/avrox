//! Command line frontend for the `avrox` code generator.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use clap::Parser;

/// Generates code from Avro schemas.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Avro schema file to generate code from.
    input: PathBuf,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    run(&cli)
}

fn run(cli: &Cli) -> Result<()> {
    let schema = fs::read_to_string(&cli.input)
        .with_context(|| format!("failed to read `{}`", cli.input.display()))?;

    let code = avrox::generate(&schema)?;

    print!("{code}");

    Ok(())
}
