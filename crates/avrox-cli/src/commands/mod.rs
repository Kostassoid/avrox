//! Subcommand implementations.

pub(crate) mod extract_schema;
pub(crate) mod generate;
pub(crate) mod read;
pub(crate) mod write;

use anyhow::Result;

use crate::cli::{Cli, Commands};

/// Runs the subcommand selected on the command line.
pub(crate) fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Generate(args) => generate::run(&args),
        Commands::ExtractSchema(args) => extract_schema::run(&args),
        Commands::Read(args) => read::run(&args),
        Commands::Write(args) => write::run(&args),
    }
}
