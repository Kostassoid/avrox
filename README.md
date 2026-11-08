<div align="center">
<img src="https://raw.githubusercontent.com/Kostassoid/avrox/main/assets/logo.svg" alt="Logo" />
</div>

# avrox

Avro code generator in the spirit of `protoc`: reads `.avsc` schemas and
produces strongly typed code with serialization boilerplate. The CLI can also
inspect and create Avro OCF files.

## Overview

| Crate | Description |
| --- | --- |
| [`avrox`](https://github.com/Kostassoid/avrox/tree/main/crates/avrox) | Library: schema parsing, validation and code generation. |
| [`avrox-cli`](https://github.com/Kostassoid/avrox/tree/main/crates/avrox-cli) | Command line frontend built on top of the library. |

Schemas may target Avro `1.10`, `1.11` or `1.12` (the default); logical types
from later versions are version-gated.

Known limitations:

- code generation targets Rust only;
- schemas mapping to colliding Rust names across namespaces are rejected;
- Avro protocols (`.avpr`) are not supported;
- `read` and `write` are exploratory tools: union branches are flattened,
  bytes and fixed values use byte arrays, decimals use strings, and not every
  detail of Avro's JSON encoding is implemented.

## CLI

### Installation

With Cargo:

```sh
cargo install avrox-cli
```

Or use an installer script, which downloads the latest prebuilt release,
installs `avrox-cli` into your Cargo bin directory and adds it to `PATH`.

macOS and Linux:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Kostassoid/avrox/releases/latest/download/avrox-cli-installer.sh | sh
```

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/Kostassoid/avrox/releases/latest/download/avrox-cli-installer.ps1 | iex"
```

Prebuilt binaries exist for Apple Silicon macOS, x86_64 and ARM64 Linux, and
x64 Windows; see the [releases page](https://github.com/Kostassoid/avrox/releases)
for archives and checksums.

### Usage

The command is `avrox-cli`, and every subcommand accepts `--help`.

```sh
avrox-cli generate path/to/schema.avsc --output-file src/gen.rs
avrox-cli generate path/to/schema.avsc --output-dir src/gen
avrox-cli extract-schema path/to/data.avro > schema.avsc
avrox-cli read path/to/data.avro
avrox-cli write path/to/new.avro path/to/schema.avsc path/to/records.jsonl
avrox-cli --help
```

The subcommands are `generate` (code generation), `extract-schema` (schema
extraction), `read` (record exploration) and `write` (record creation).

### generate

`generate` reads a schema and writes generated code:

```
avrox-cli generate <SCHEMA> (--output-file <FILE> | --output-dir <DIR>) [OPTIONS]
```

Output — exactly one is required:

- `--output-file <FILE>`: single-file layout, with everything written to the
  given path;
- `--output-dir <DIR>`: module layout, with a directory dedicated to generated
  types and one file per named type.

Target:

- `--language <LANG>`: output language, currently only `rust` (default);
- `--avro-version <VERSION>`: `1.10`, `1.11` or `1.12` (default).

Rust types (`--rust-types`):

- `avro` (default): relies on the official [`apache-avro`](https://docs.rs/apache-avro)
  crate — `bytes`/`fixed` use `serde_bytes`, and `uuid`/`decimal`/`duration`
  become `apache_avro::Uuid`/`Decimal`/`Duration`. Because
  `apache_avro::Uuid` serializes as bytes in binary mode, `uuid` over `string`
  gets generated `serde` adapters that encode it as a UUID string;
- `std`: only standard library types plus types generated into the output (a
  `Bytes` newtype and one newtype per `fixed` type, with `serde`
  implementations that serialize as Avro bytes). Non-time logical types keep
  their underlying primitives.

Rust time (`--rust-time`):

- `chrono` (default): time-related logical types become `chrono::NaiveDate`,
  `chrono::NaiveTime`, `chrono::DateTime<Utc>` and `chrono::NaiveDateTime`,
  with field-level `serde` adapters that encode them as the Avro integers of
  their logical types. Requires `chrono` 0.4.40 or later with the `serde`
  feature;
- `primitives`: time-related logical types (`date`, `time-*`, `timestamp-*`,
  `local-timestamp-*`) keep their underlying integers (`i32`/`i64`), documented
  on the fields carrying them. No extra dependencies;
- `std-time`: timestamps become `std::time::SystemTime` and times-of-day
  `std::time::Duration`, with field-level `serde` adapters that encode them as
  the Avro integers of their logical types. `date` has no standard library
  representation and stays an `i32`. No extra dependencies.

Rust derives (`--rust-derive <DERIVE>`):

- adds a derive to generated records, enums and union enums; repeatable, also
  accepted as `--rust-derives`. Use short names for common derives (`Hash`,
  `Display`) and fully qualified paths for derives from other crates
  (`derive_more::Display`). Derives the generator already emits are ignored.

#### Generated Rust code

Generated code integrates with [`apache-avro`](https://docs.rs/apache-avro)
through `serde`:

- records become structs, enums become enums;
- nullable unions (`["null", T]`) become `Option<T>`; other unions become
  generated enums with one newtype variant per branch, in branch order;
- every branch of a union enum gets a `From` implementation that lifts the
  value into it, so application code rarely has to name the enum; branches
  whose lifted types collide, such as two `fixed` types of the same size in
  `avro` mode, are skipped;
- directly recursive references are boxed;
- logical types are documented on the fields and types carrying them;
- in `avro` mode, `uuid` over `string` becomes `apache_avro::Uuid` with a
  generated `serde` adapter, so it encodes as a UUID string;
- `bytes` and `fixed` follow `--rust-types`, and time-related logical types
  follow `--rust-time` (see above).

Logical types that need `serde` adapters use generated newtypes when nested
inside arrays or maps, because `serde` cannot attach field attributes to
collection elements.

`std` mode needs only `serde` with `derive`; `avro` mode needs `serde`,
`serde_bytes` and `apache-avro`. Both round-trip through `apache-avro`'s binary
codec, and both need `chrono` 0.4.40 or later with the `serde` feature when
`--rust-time` is `chrono` (the default) — `primitives` and `std-time` need no
extra dependencies.

### extract-schema

`extract-schema` prints the schema stored in the header of an Avro OCF file
(`.avro`) to standard output and nothing else, so the result can be redirected
or piped:

```sh
avrox-cli extract-schema path/to/data.avro > schema.avsc
```

The output is byte-identical to the stored schema. Non-container files are rejected.

### read

`read` prints every record of an Avro OCF file as JSON, one record per line:

```sh
avrox-cli read path/to/data.avro
avrox-cli read --human-readable path/to/data.avro
```

The output is meant for exploration and debugging, so some Avro details are
flattened: union branches are unwrapped, enums print their symbol, UUIDs,
decimals and big decimals print as strings, and bytes and fixed values print as
arrays of numbers. Time-related logical types print their underlying integers
by default, and durations print as `{"months", "days", "millis"}` objects.
`--human-readable` instead renders dates as ISO 8601 dates, times and
timestamps as ISO 8601 date-times, and durations as ISO 8601 durations.

### write

`write` creates an Avro OCF file from JSON records, one record per line. The
output and schema paths are required; the JSONL input path is optional and
standard input is used when it is omitted:

```sh
avrox-cli write path/to/new.avro path/to/schema.avsc records.jsonl
avrox-cli write --human-readable path/to/new.avro path/to/schema.avsc < records.jsonl
```

Every record must match the schema. Missing fields are filled from the schema
default or, for nullable fields, with `null`. Missing required fields, unknown
fields, invalid enum symbols and type mismatches fail immediately with the
offending line and JSON path. With `--human-readable`, dates, times, timestamps
and durations are read from the same ISO 8601 formats produced by
`read --human-readable`.

## Library

Add the `avrox` crate from crates.io as a dependency:

```sh
cargo add avrox
```

Then parse the schema and call `generate` with a `Settings` value:

```rust
let text = std::fs::read_to_string("path/to/schema.avsc")?;
let schema: avrox::Schema = text.parse()?;
let files = avrox::generate(&schema, &avrox::Settings::default())?;
```

Parsing defaults to Avro 1.12, the latest supported version. Use
`Schema::parse` to parse against another specification version:

```rust
use avrox::{AvroVersion, Schema};

let schema = Schema::parse(&text, AvroVersion::V1_11)?;
```

The result is a list of generated files, each pairing a relative path with its
contents:

```rust
for file in &files {
    println!("{} ({} bytes)", file.path, file.contents.len());
}
```

### Building settings

`Settings` controls the target language, output layout and language-specific
options. `Settings::new()` starts from the defaults, and the builder methods
return the modified value, so settings can be built in a single expression:

```rust
use avrox::{Layout, RustTime, RustTypes, Settings};

let settings = Settings::new()
    .layout(Layout::Module)
    .rust_types(RustTypes::Std)
    .rust_time(RustTime::Chrono)
    .rust_derives(["Hash", "derive_more::Display"]);
```

`RustOptions` has the same builder style and can be assembled separately:

```rust
use avrox::{RustOptions, RustTime, RustTypes, Settings};

let rust = RustOptions::new()
    .types(RustTypes::ApacheAvro)
    .time(RustTime::StdTime)
    .derive("Hash");

let settings = Settings::new().rust(rust);
```

### Writing generated files

For a single output file, write the first generated file to the desired path:

```rust
use std::fs;

let settings = avrox::Settings::default();
let files = avrox::generate(&schema, &settings)?;
if let Some(file) = files.first() {
    fs::write("src/generated.rs", &file.contents)?;
}
```

With the module layout, write every generated file below an output directory,
creating subdirectories as needed:

```rust
use std::{fs, path::Path};

fn write_module(text: &str, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let schema: avrox::Schema = text.parse()?;
    let settings = avrox::Settings::new().layout(avrox::Layout::Module);
    for file in avrox::generate(&schema, &settings)? {
        let path = output.join(&file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, file.contents)?;
    }
    Ok(())
}
```

Schemas without named types (for example a bare primitive or an array of
primitives) produce no files, so `generate` may return an empty list.

### Generated header

Every generated file starts with a header comment recording the library version
and the settings used:

```rust
//! Generated by avrox 1.2.0 from an Avro schema. DO NOT EDIT.
//!
//! Settings used for generation:
//!
//! - language: rust
//! - avro version: 1.12
//! - layout: module
//! - rust types: std
//! - rust time: chrono
//! - rust derives: Hash, derive_more::Display
```

### Using generated models with Apache Avro

Generated records and enums implement `serde::Serialize` and
`serde::Deserialize`, so they can be used directly with the
[`apache-avro`](https://docs.rs/apache-avro) crate. A value read from a source (an Avro
file, a Kafka topic, etc.) is encoded with the writer schema, so before deserializing it into a
generated model (the reader schema) it must be resolved with
`apache_avro::types::Value::resolve` against the schema the model was generated
from:

```rust
use apache_avro::{Reader, from_value};
use std::{fs::File, io::BufReader};

let schema_text = std::fs::read_to_string("schema.avsc")?;
let read_schema = apache_avro::Schema::parse_str(&schema_text)?;

let file = File::open("data.avro")?;
let reader = Reader::new(BufReader::new(file))?;

for value in reader {
    let value = value?;
    // Adapt the value from the file's writer schema to the model's schema.
    let value = value.resolve(&read_schema)?;
    let model: MyRecord = from_value(&value)?;
    // use `model` ...
}
```

`Reader` can perform the same resolution internally when given the reader
schema (`Reader::builder(file).reader_schema(&read_schema)`), but
`into_deser_iter` does not resolve schemas and panics if the writer and reader
schemas differ.

Writing works in the opposite direction: serialize the model to a `Value`,
resolve it against the schema of the target file or a topic. In practice, the writer usually controls the schema so 
there is no need to resolve it, the generated models can be serialized directly.

## Contributing

Contributions are welcome. Common commands are available as
[`just`](https://just.systems) recipes — run `just --list` to see them all:

```sh
just ci      # run the same checks as CI (fmt, clippy, build, test)
just test    # run all tests
just bless   # regenerate the expected outputs under tests/cases
just run generate path/to/schema.avsc --output-dir src/gen  # run the CLI
```

Without `just`, the equivalent raw commands are:

```sh
cargo build --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all --check
```

### Testing

Testing uses three layers:

- unit tests for the parser, naming and generator internals;
- black-box snapshot tests under `tests/cases`: every case is a directory with
  a `schema.avsc`, an optional `settings.json`, and either an `expected/`
  directory with the expected generated files or an `error.txt` with the
  expected error;
- round-trip verification against `apache-avro` in `examples` directory. The application there reads a sample Avro data file using generated types, then serializes the records back to an in-memory Avro file and verifies that the round trip preserves every value. It's not a part of the CI workflow.

The snapshot expectations can be regenerated with `AVROX_BLESS=1 cargo test`.

### Releasing

Releases are built by [dist](https://axodotdev.github.io/cargo-dist), cut with
[cargo-release](https://github.com/crate-ci/cargo-release) and use
[git-cliff](https://git-cliff.org) for changelog capture. Prerequisites:
`cargo install cargo-release` and `git-cliff` on your `PATH`.

1. Keep the `## [Unreleased]` section of `CHANGELOG.md` up to date: write notes
   by hand, or run `just changelog` to print the commits since the last tag and
   paste in what you want to include.
2. Run `just release 1.1.0` — a dry run; add `--execute` to actually do it.
   This bumps all workspace versions in lockstep, rotates the `Unreleased`
   section into `## [1.1.0] - <date>`, commits, creates the `v1.1.0` tag and
   pushes it.
3. Pushing the tag triggers `.github/workflows/release.yml`, which builds
   `avrox-cli` for all platforms, creates the GitHub Release with the matching changelog entry as
   its notes, and uploads the archives, shell/PowerShell installers and SHA-256
   checksums.
4. The same release publishes the `avrox` and `avrox-cli` crates to crates.io
   through `.github/workflows/publish-crates.yml`. This requires a
   `CARGO_REGISTRY_TOKEN` repository secret with permission to publish both
   crates. The workflow can also be run manually from the Actions tab to
   publish the current workspace version.

## License

MIT
