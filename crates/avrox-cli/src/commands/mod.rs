//! Subcommand implementations.

pub(crate) mod generate;

use anyhow::Result;

use crate::cli::{Cli, Commands};

/// Runs the subcommand selected on the command line.
pub(crate) fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Generate(args) => generate::run(&args),
    }
}
