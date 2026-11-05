//! Command line frontend for the `avrox` code generator.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use avrox::{AvroVersion, Language, Layout, RustOptions, RustTime, RustTypes, Settings};
use clap::{Parser, ValueEnum};

/// Generates code from Avro schemas.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Avro schema file to generate code from.
    input: PathBuf,

    /// Output language.
    #[arg(short, long, value_enum, default_value = "rust")]
    language: LanguageArg,

    /// Avro specification version of the schema.
    #[arg(long, value_enum, default_value = "1.12")]
    avro_version: VersionArg,

    /// How the generated code is split into files.
    #[arg(long, value_enum, default_value = "single")]
    layout: LayoutArg,

    /// Which types the generated Rust code uses.
    #[arg(long, value_enum, default_value = "avro")]
    rust_types: TypesArg,

    /// How time-related logical types are represented in Rust.
    #[arg(long, value_enum, default_value = "primitives")]
    rust_time: TimeArg,

    /// Directory to write generated files into instead of printing them to
    /// stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,
}

#[derive(Clone, Debug, ValueEnum)]
enum LanguageArg {
    #[value(name = "rust")]
    Rust,
}

impl From<LanguageArg> for Language {
    fn from(value: LanguageArg) -> Self {
        match value {
            LanguageArg::Rust => Self::Rust,
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
enum LayoutArg {
    Single,
    Module,
}

impl From<LayoutArg> for Layout {
    fn from(value: LayoutArg) -> Self {
        match value {
            LayoutArg::Single => Self::SingleFile,
            LayoutArg::Module => Self::Module,
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
enum TypesArg {
    #[value(name = "avro")]
    ApacheAvro,
    Std,
}

impl From<TypesArg> for RustTypes {
    fn from(value: TypesArg) -> Self {
        match value {
            TypesArg::ApacheAvro => Self::ApacheAvro,
            TypesArg::Std => Self::Std,
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
enum TimeArg {
    Primitives,
    StdTime,
    Chrono,
}

impl From<TimeArg> for RustTime {
    fn from(value: TimeArg) -> Self {
        match value {
            TimeArg::Primitives => Self::Primitives,
            TimeArg::StdTime => Self::StdTime,
            TimeArg::Chrono => Self::Chrono,
        }
    }
}

#[derive(Clone, Debug, ValueEnum)]
enum VersionArg {
    #[value(name = "1.10")]
    V1_10,
    #[value(name = "1.11")]
    V1_11,
    #[value(name = "1.12")]
    V1_12,
}

impl From<VersionArg> for AvroVersion {
    fn from(value: VersionArg) -> Self {
        match value {
            VersionArg::V1_10 => Self::V1_10,
            VersionArg::V1_11 => Self::V1_11,
            VersionArg::V1_12 => Self::V1_12,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    run(&cli)
}

fn run(cli: &Cli) -> Result<()> {
    let schema = fs::read_to_string(&cli.input)
        .with_context(|| format!("failed to read `{}`", cli.input.display()))?;
    let settings = Settings {
        language: cli.language.clone().into(),
        avro_version: cli.avro_version.clone().into(),
        layout: cli.layout.clone().into(),
        rust: RustOptions {
            types: cli.rust_types.clone().into(),
            time: cli.rust_time.clone().into(),
        },
    };
    let files = avrox::generate(&schema, &settings)
        .with_context(|| format!("failed to generate code from `{}`", cli.input.display()))?;
    write_output(cli, &files)
}

fn write_output(cli: &Cli, files: &[avrox::GeneratedFile]) -> Result<()> {
    if files.is_empty() {
        println!("no types to generate for `{}`", cli.input.display());
        return Ok(());
    }
    match &cli.output {
        Some(directory) => {
            fs::create_dir_all(directory)
                .with_context(|| format!("failed to create `{}`", directory.display()))?;
            for file in files {
                let path = directory.join(&file.path);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("failed to create `{}`", parent.display()))?;
                }
                fs::write(&path, &file.contents)
                    .with_context(|| format!("failed to write `{}`", path.display()))?;
                println!("generated {}", path.display());
            }
        }
        None => {
            for (index, file) in files.iter().enumerate() {
                if files.len() > 1 {
                    if index > 0 {
                        println!();
                    }
                    println!("// ===== {} =====", file.path);
                }
                print!("{}", file.contents);
            }
        }
    }
    Ok(())
}
