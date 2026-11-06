//! Command surface: the root parser and the list of subcommands.

use clap::Parser;

use crate::commands::generate::Generate;

/// Command line frontend for the `avrox` code generator.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

/// Available subcommands.
#[derive(Debug, clap::Subcommand)]
pub(crate) enum Commands {
    /// Generate code from an Avro schema.
    Generate(Generate),
}
