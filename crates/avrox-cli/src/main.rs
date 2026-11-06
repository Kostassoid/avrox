//! Command line frontend for the `avrox` code generator.

mod cli;
mod commands;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    commands::run(cli::Cli::parse())
}
