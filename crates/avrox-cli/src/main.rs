//! Command line frontend for the `avrox` code generator.

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use avrox::{AvroVersion, Language, Layout, RustOptions, RustTime, RustTypes, Settings};
use clap::{ArgGroup, Parser, ValueEnum};

/// Generates code from Avro schemas.
#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
#[command(group(
    ArgGroup::new("output")
        .required(true)
        .multiple(false)
        .args(["output_file", "output_dir"]),
))]
struct Cli {
    /// Avro schema file to generate code from.
    input: PathBuf,

    /// Output language.
    #[arg(short, long, value_enum, default_value = "rust")]
    language: LanguageArg,

    /// Avro specification version of the schema.
    #[arg(long, value_enum, default_value = "1.12")]
    avro_version: VersionArg,

    /// Which types the generated Rust code uses.
    #[arg(long, value_enum, default_value = "avro")]
    rust_types: TypesArg,

    /// How time-related logical types are represented in Rust.
    #[arg(long, value_enum, default_value = "primitives")]
    rust_time: TimeArg,

    /// File to write the generated code to, ignoring the file name proposed
    /// by the generator. Selects the single-file layout.
    #[arg(long)]
    output_file: Option<PathBuf>,

    /// Directory to write the generated files into. Selects the module
    /// layout.
    #[arg(long)]
    output_dir: Option<PathBuf>,
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
    let (layout, output) = match (&cli.output_file, &cli.output_dir) {
        (Some(file), None) => (Layout::SingleFile, Output::File(file.clone())),
        (None, Some(directory)) => (Layout::Module, Output::Directory(directory.clone())),
        // clap enforces that exactly one of the two is given.
        _ => unreachable!("exactly one of `--output-file`/`--output-dir` must be given"),
    };
    let settings = Settings {
        language: cli.language.clone().into(),
        avro_version: cli.avro_version.clone().into(),
        layout,
        rust: RustOptions {
            types: cli.rust_types.clone().into(),
            time: cli.rust_time.clone().into(),
        },
    };
    let files = avrox::generate(&schema, &settings)
        .with_context(|| format!("failed to generate code from `{}`", cli.input.display()))?;
    write_output(cli, &output, &files)
}

enum Output {
    File(PathBuf),
    Directory(PathBuf),
}

fn write_output(cli: &Cli, output: &Output, files: &[avrox::GeneratedFile]) -> Result<()> {
    if files.is_empty() {
        println!("no types to generate for `{}`", cli.input.display());
        return Ok(());
    }
    match output {
        Output::File(path) => {
            let file = &files[0];
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create `{}`", parent.display()))?;
            }
            fs::write(path, &file.contents)
                .with_context(|| format!("failed to write `{}`", path.display()))?;
            println!("generated {}", path.display());
        }
        Output::Directory(directory) => {
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
    }
    Ok(())
}
