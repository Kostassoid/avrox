# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
