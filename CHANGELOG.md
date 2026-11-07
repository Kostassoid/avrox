# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.6.0] - 2026-11-07

### Added

- Generated union enums now implement `From` for every branch type, so a value
  can be lifted into the union without naming the enum. Branches that would
  produce conflicting implementations, such as two `fixed` types of the same
  size in `avro` mode, are skipped.

### Changed

- `avrox::generate` now takes a parsed `avrox::Schema` instead of a JSON
  string. Build the schema first — with `"...".parse()` for the default Avro
  1.12 or with `Schema::parse(source, version)` to pick another specification
  version — and pass a reference to `generate`.
- The Avro specification version moved from `Settings` to the schema:
  `Settings::avro_version` and its builder method were removed, `Schema`
  records the version it was parsed against, and the generated header reads it
  from there.
- `avrox::schema::Document` was renamed to `avrox::schema::Schema` (re-exported
  as `avrox::Schema`), and the former `avrox::schema::Schema` enum is now
  `avrox::schema::SchemaNode`. The free `avrox::schema::parse` function was
  replaced by the `Schema::parse` and `FromStr` constructors.
- `--rust-time` and `RustOptions::time` now default to `chrono` instead of
  `primitives`, so time-related logical types generate bare `chrono` types with
  field-level `serde` adapters out of the box. Generated code then needs
  `chrono` 0.4.40 or later with the `serde` feature; pass
  `--rust-time primitives` (or `RustTime::Primitives`) for the previous
  behaviour.

## [1.5.0] - 2026-11-07

### Changed

- `Settings` and `RustOptions` now expose only functional-style builders, with
  the `with_` prefix removed (`with_layout` becomes `layout`, `with_types`
  becomes `types`, and so on). The in-place `set_*` and `add_*` methods have
  been removed.
- `avrox-cli read` now renders `decimal` values as exact decimal strings using
  the schema scale instead of arrays of bytes, and `avrox-cli write` accepts
  those strings alongside the previous byte-array form.
- `--rust-time chrono` now maps time-related logical types to their bare
  `chrono` types (`NaiveDate`, `NaiveTime`, `DateTime<Utc>`,
  `NaiveDateTime`) and attaches field-level `serde` adapters instead of
  generating wrapper newtypes. This removes the extra boilerplate and keeps
  the generated domain types clean. Logical types nested inside arrays or
  maps still use generated newtypes because `serde` cannot attach field
  attributes to collection elements.
- `--rust-time std-time` now maps timestamps to bare
  `std::time::SystemTime` values and times-of-day to `std::time::Duration`,
  with field-level `serde` adapters. Logical types nested inside arrays or
  maps still use generated newtypes, as in `chrono` mode.

## [1.4.1] - 2026-11-06

### Added

- The `avrox` library and `avrox-cli` executable are now published to
  crates.io on every release, so the CLI can be installed with
  `cargo install avrox-cli` and the library added with `cargo add avrox`.

## [1.4.0] - 2026-11-06

## [1.3.0] - 2026-11-06

### Added

- `--rust-derive` CLI option and `RustOptions::derives` setting for adding
  derives to generated Rust records, enums and union enums. Short names like
  `Hash` and fully qualified paths like `derive_more::Display` are accepted.
- Builder-style methods for `Settings` and `RustOptions` (`Settings::new`,
  `with_layout`, `with_rust_types`, `with_rust_derives`, `set_*` and friends),
  making it easier to construct and adjust generation settings.
- Generated files now carry a header comment with the `avrox` version and the
  settings used for generation.
- Shell and PowerShell installers for `avrox-cli`, letting users download the
  latest release and install it with a single command.

## [1.2.0] - 2026-11-06

### Added

- `avrox-cli extract-schema` subcommand: prints the schema stored in an Avro
  data file (`.avro`) to stdout and nothing else, reading only the file
  header so files of any size can be processed.
- `avrox-cli read` subcommand: prints the records of an Avro data file as
  JSON lines for exploration and debugging, with an optional
  `--human-readable` mode for dates, times, timestamps and durations.
- `avrox-cli write` subcommand: creates an Avro data file from a schema and
  JSON lines read from a file or stdin, validating each record against the
  schema and supporting the same `--human-readable` logical-type formats as
  `read`.

### Changed

- `avrox-cli` is now organized as subcommands: code generation moved under
  `avrox generate <schema.avsc> ...`, taking the former top-level arguments.
  Invoking the tool with a bare schema path is no longer accepted.

## [1.1.0] - 2026-11-05

### Added

- `avrox` library: an Avro schema parser and validator with full name and
  namespace resolution, union and default-value validation, version-gated
  logical types (Avro 1.10, 1.11 and 1.12) and an intermediate
  representation of schema documents.
- Rust code generation: records become structs, enums become enums, fixed
  types become byte arrays, nullable unions become `Option` and other unions
  become generated enums; recursive references are boxed, `serde` renames and
  Rust keyword field names are handled, and logical types are documented on
  the fields carrying them.
- Two output layouts (`single` file and `module` directory with `mod.rs`) and
  two type modes: `avro` (relying on `apache-avro` and `serde_bytes` types)
  and `std` (standard library types plus generated helpers that still
  round-trip through `apache-avro`).
- `avrox-cli` command line frontend with `--language`, `--avro-version`,
  `--rust-types` and `--rust-time` options; exactly one of `--output-file`
  (single-file layout, treated as a file path) or `--output-dir` (module
  layout) must be given and decides where the generated code is written.
- Black-box snapshot tests under `tests/cases`, unit tests for the parser and
  generator, and round-trip verification against `apache-avro`.
- CI workflow, `Justfile` with common development commands and a `dist`-based
  release pipeline building `avrox-cli` binaries for Windows, Linux (x86_64
  and aarch64) and macOS (aarch64).
