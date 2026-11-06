<div align="center">
<img src="assets/logo.svg" alt="Logo" />
</div>

# avrox

Avro code generator, in the spirit of `protoc`: takes `.avsc` schemas and
produces strongly typed code with serialization boilerplate for a target
language. The workspace also ships a CLI for generating code and inspecting or
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

All examples run through Cargo. If you have the `avrox-cli` binary installed,
drop the `cargo run -p avrox-cli --` prefix. Every command accepts `--help`.

```sh
cargo run -p avrox-cli -- generate path/to/schema.avsc --output-file src/gen.rs
cargo run -p avrox-cli -- generate path/to/schema.avsc --output-dir src/gen
cargo run -p avrox-cli -- extract-schema path/to/data.avro > schema.avsc
cargo run -p avrox-cli -- read path/to/data.avro
cargo run -p avrox-cli -- write path/to/new.avro path/to/schema.avsc path/to/records.jsonl
cargo run -p avrox-cli -- --help
```

The CLI is organized as subcommands: `generate` (code generation),
`extract-schema` (schema extraction from data files), `read` (record
exploration) and `write` (record creation).

### Generate code

`generate` takes a schema and writes generated code. Its options include
`--language` (currently `rust`), `--avro-version` (`1.10`, `1.11` or `1.12`,
the default), `--rust-types` and `--rust-time`. Exactly one of the two output
options must be given:

- `--output-file <FILE>`: single-file layout — everything in one file,
  written to the given path; the file name proposed by the generator is
  ignored;
- `--output-dir <DIR>`: module layout — one file per named type plus an
  index file declaring and re-exporting them. For Rust (currently the only
  supported language) that index file is a `mod.rs` which also holds the
  generated union enums, so the result can be used as a module directory
  as is.
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
cargo run -p avrox-cli -- extract-schema path/to/data.avro > schema.avsc
```

It stops reading as soon as it has reached the schema, so data files of any
size work. The output is byte-identical to the schema as stored in the file.
Non-container files are rejected.

### Read records

`read` prints every record of an Avro data file as JSON, one record per line:

```sh
cargo run -p avrox-cli -- read path/to/data.avro
cargo run -p avrox-cli -- read --human-readable path/to/data.avro
```

The output is meant for exploration and debugging, so some Avro details are
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
cargo run -p avrox-cli -- write path/to/new.avro path/to/schema.avsc records.jsonl
cargo run -p avrox-cli -- write --human-readable path/to/new.avro path/to/schema.avsc < records.jsonl
```

Every record must match the schema. Requirements and defaults are enforced:
missing fields are filled from the schema default or, for nullable fields,
with `null`; missing required fields, unknown fields, invalid enum symbols and
type mismatches fail immediately with the offending line and JSON path. With
`--human-readable`, dates, times, timestamps and durations are read from the
same ISO 8601 formats produced by `read --human-readable`.

## Library

Add the `avrox` crate as a dependency and call `generate`:

```rust
let files = avrox::generate(schema_json, &avrox::Settings::default())?;
```

`Settings` controls the target language, Avro version, output layout and
language-specific options. The result is a list of generated files, each
pairing a relative path with its contents.

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
   as its notes, and uploads the archives with SHA-256 checksums.

## License

MIT
