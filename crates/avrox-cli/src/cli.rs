//! Command surface: the root parser and the list of subcommands.

use clap::Parser;

use crate::commands::{
    extract_schema::ExtractSchema, generate::Generate, read::Read, write::Write,
};

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
    /// Extract the schema from an Avro data file and print it to stdout.
    ExtractSchema(ExtractSchema),
    /// Read records from an Avro data file and print them as JSON to stdout,
    /// one record per line.
    Read(Read),
    /// Write JSON records to a new Avro data file according to a schema.
    Write(Write),
}
