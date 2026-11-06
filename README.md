<div align="center">
<img src="assets/logo.svg" alt="Logo" />
</div>

# avrox

Avro code generator, in the spirit of `protoc`: takes `.avsc` schemas and
produces strongly typed code with serialization boilerplate for a target
language. Additionally, the CLI provides a command-line interface for generating code and inspecting or
creating Avro data files.

## Overview

| Crate | Description |
| --- | --- |
| [`avrox`](crates/avrox) | Library: schema parsing, validation and code generation. |
| [`avrox-cli`](crates/avrox-cli) | Command line frontend built on top of the library. |

Schemas can target Avro `1.10`, `1.11` or `1.12` (the default). Logical types
introduced in later versions are version-gated.

Known limitations:

- code generation currently targets Rust only;
- schemas whose types map to colliding Rust names across namespaces are
  rejected;
- Avro protocols (`.avpr`) are not supported;
- uuid logical types on `string` stay `String` in `avro` mode because
  `apache_avro::Uuid` serializes as bytes in binary mode, which a string-based
  uuid schema rejects;
- `read` and `write` focus on exploration and debugging: union branches are
  flattened, bytes/fixed/decimal values use byte arrays, BigDecimal values use
  strings, and not every detail of Avro's JSON encoding is implemented.

## CLI

### Installation

The installer scripts download the latest release for your platform, install
`avrox-cli` into your Cargo bin directory and add it to your `PATH`.

macOS and Linux:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Kostassoid/avrox/releases/latest/download/avrox-cli-installer.sh | sh
```

Windows (PowerShell):

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/Kostassoid/avrox/releases/latest/download/avrox-cli-installer.ps1 | iex"
```

Prebuilt binaries are available for Apple Silicon macOS, x86_64 and ARM64
Linux, and x64 Windows. See the
[releases page](https://github.com/Kostassoid/avrox/releases) for archives and
checksums.

### Usage

The installed command is `avrox-cli`. Every subcommand accepts `--help`.

```sh
avrox-cli generate path/to/schema.avsc --output-file src/gen.rs
avrox-cli generate path/to/schema.avsc --output-dir src/gen
avrox-cli extract-schema path/to/data.avro > schema.avsc
avrox-cli read path/to/data.avro
avrox-cli write path/to/new.avro path/to/schema.avsc path/to/records.jsonl
avrox-cli --help
```

The CLI is organized as subcommands: `generate` (code generation),
`extract-schema` (schema extraction from data files), `read` (record
exploration) and `write` (record creation).

### Generate code

`generate` takes a schema and writes generated code. Its options include
`--language` (currently `rust`), `--avro-version` (`1.10`, `1.11` or `1.12`,
the default), `--rust-types`, `--rust-time` and `--rust-derive`. Exactly one of
the two output options must be given:

- `--output-file <FILE>`: single-file layout — everything in one file,
  written to the given path;
- `--output-dir <DIR>`: module layout — target directory is fully dedicated to generated types, allowing for 
  better readability. Each named type is written to a separate file.
- `--rust-types avro` (default): generated types rely on the official
  [`apache-avro`](https://docs.rs/apache-avro) crate — `bytes`/`fixed` use
  `serde_bytes`, and the `uuid`/`decimal`/`duration` logical types become
  `apache_avro::Uuid`/`Decimal`/`Duration`;
- `--rust-types std`: only standard library types plus types generated into
  the output (a `Bytes` newtype and one newtype per `fixed` type, with `serde`
  implementations that serialize as Avro bytes). Logical types keep their
  underlying primitives. The generated code needs only `serde` to compile
  and still round-trips through `apache-avro`.
- `--rust-time primitives` (default): time-related logical types (`date`,
  `time-*`, `timestamp-*`, `local-timestamp-*`) keep their underlying
  integers (`i32`/`i64`), documented on the fields carrying them;
- `--rust-time std-time`: timestamps become newtypes wrapping
  `std::time::SystemTime` and times-of-day newtypes wrapping
  `std::time::Duration`; `date` has no standard library representation and
  stays an `i32`. Needs no extra dependencies;
- `--rust-time chrono`: newtypes wrapping `chrono::NaiveDate`,
  `chrono::NaiveTime`, `chrono::DateTime<Utc>` and `chrono::NaiveDateTime`,
  with `serde` implementations that encode them as the Avro integers of their
  logical types. The consuming crate needs `chrono` 0.4.40 or later.
- `--rust-derive <DERIVE>` (repeatable, also accepted as `--rust-derives`):
  adds a derive to generated records, enums and union enums. Common derives
  can use their short name (`Hash`, `Display`), while derives from other
  crates should use their fully qualified path (`derive_more::Display`).
  Derives already emitted by the generator are not added twice.

#### Generated Rust code

The generated code integrates with the [`apache-avro`](https://docs.rs/apache-avro)
crate through `serde`:

- records become structs, enums become enums;
- nullable unions (`["null", T]`) become `Option<T>`, other unions become
  generated enums with one newtype variant per branch, in branch order;
- recursive references in directly nested positions are boxed;
- logical types are documented on the fields and types carrying them, and
  time-related ones follow the `--rust-time` setting (see above);
- `bytes` and `fixed` representations depend on the `--rust-types` mode (see
  above): `serde_bytes` types in `avro` mode, generated newtypes with
  byte-oriented `serde` implementations in `std` mode.

Both modes round-trip through `apache-avro`'s binary codec; `std` mode
requires only `serde` (with `derive`) as a direct dependency, `avro` mode
requires `serde`, `serde_bytes` and `apache-avro`.

### Extract a schema

`extract-schema` prints the schema stored in the header of an Avro data file
(`.avro`) to standard output and nothing else, so it can be redirected or
piped:

```sh
avrox-cli extract-schema path/to/data.avro > schema.avsc
```

It stops reading as soon as it has reached the schema, so data files of any
size work. The output is byte-identical to the schema as stored in the file.
Non-container files are rejected.

### Read records

`read` prints every record of an Avro data file as JSON, one record per line:

```sh
avrox-cli read path/to/data.avro
avrox-cli read --human-readable path/to/data.avro
```

The output is meant mostly for exploration and debugging, so some Avro details are
flattened: union branches are unwrapped, enums print their symbol, UUIDs print
as strings, big decimals print as strings, and bytes, fixed and decimal values
print as arrays of numbers. Time-related logical types print their underlying
integers by default and durations print as `{"months", "days", "millis"}`
objects; `--human-readable` renders dates as ISO 8601 dates, times and
timestamps as ISO 8601 date-times and durations as ISO 8601 durations instead.

### Write records

`write` creates an Avro data file from JSON records, one record per line. The
output path and schema path are required; the JSONL input path is optional and
standard input is used when it is omitted:

```sh
avrox-cli write path/to/new.avro path/to/schema.avsc records.jsonl
avrox-cli write --human-readable path/to/new.avro path/to/schema.avsc < records.jsonl
```

Every record must match the schema. Requirements and defaults are enforced:
missing fields are filled from the schema default or, for nullable fields,
with `null`; missing required fields, unknown fields, invalid enum symbols and
type mismatches fail immediately with the offending line and JSON path. With
`--human-readable`, dates, times, timestamps and durations are read from the
same ISO 8601 formats produced by `read --human-readable`.

## Library

Add the `avrox` crate as a dependency and call `generate` with a schema in
JSON format and a `Settings` value:

```rust
let schema = std::fs::read_to_string("path/to/schema.avsc")?;
let files = avrox::generate(&schema, &avrox::Settings::default())?;
```

The result is a list of generated files, each pairing a relative path with its
contents:

```rust
for file in &files {
    println!("{} ({} bytes)", file.path, file.contents.len());
}
```

### Building settings

`Settings` controls the target language, Avro version, output layout and
language-specific options. `Settings::new()` starts from the defaults, and
`with_*` methods return the modified value, so settings can be built in a
single expression:

```rust
use avrox::{AvroVersion, Layout, RustTime, RustTypes, Settings};

let settings = Settings::new()
    .with_layout(Layout::Module)
    .with_avro_version(AvroVersion::V1_11)
    .with_rust_types(RustTypes::Std)
    .with_rust_time(RustTime::Chrono)
    .with_rust_derives(["Hash", "derive_more::Display"]);
```

`RustOptions` has the same builder style and can be assembled separately:

```rust
use avrox::{RustOptions, RustTime, RustTypes, Settings};

let rust = RustOptions::new()
    .with_types(RustTypes::ApacheAvro)
    .with_time(RustTime::StdTime)
    .with_derive("Hash");

let settings = Settings::new().with_rust(rust);
```

Settings can also be modified in place through the `set_*` methods and
`add_rust_derive`:

```rust
let mut settings = Settings::default();
settings
    .set_layout(Layout::Module)
    .set_rust_types(RustTypes::Std)
    .add_rust_derive("Hash");
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

fn write_module(schema: &str, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let settings = avrox::Settings::new().with_layout(avrox::Layout::Module);
    for file in avrox::generate(schema, &settings)? {
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

Every generated file starts with a header comment that records the library
version and the settings used to generate it:

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
  a `schema.avsc`, optional `settings.json`, and either an `expected/` directory
  with the expected generated files or an `error.txt` with the expected error;
- the snapshot expectations can be regenerated with `AVROX_BLESS=1 cargo test`.

### Releasing

Releases are built by [dist](https://axodotdev.github.io/cargo-dist), cut
with [cargo-release](https://github.com/crate-ci/cargo-release) and use
[git-cliff](https://git-cliff.org) for changelog capture. Prerequisites:
`cargo install cargo-release` and `git-cliff` on your `PATH`.

1. Keep the `## [Unreleased]` section of `CHANGELOG.md` up to date: write
   notes by hand, or run `just changelog` to print the commits since the
   last tag and paste in what you want to include.
2. Run `just release 1.1.0` — a dry run; add `--execute` to actually do it.
   This bumps all workspace versions in lockstep, rotates the `Unreleased`
   section into `## [1.1.0] - <date>`, commits, creates the `v1.1.0` tag and
   pushes it.
3. Pushing the tag triggers `.github/workflows/release.yml`, which builds
   `avrox-cli` for Windows (x86_64), Linux (x86_64 and aarch64) and macOS
   (aarch64), creates the GitHub Release with the matching changelog entry
   as its notes, and uploads the archives, shell/PowerShell installers and
   SHA-256 checksums.

## License

MIT
