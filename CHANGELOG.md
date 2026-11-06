# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
