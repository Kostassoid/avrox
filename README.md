<div align="center">
<img src="assets/logo.svg" alt="Logo" />
</div>

# avrox

Avro code generator, in the spirit of `protoc`: takes `.avsc` schemas and
produces strongly typed code with serialization boilerplate for a target
language.

## Workspace

| Crate | Description |
| --- | --- |
| [`avrox`](crates/avrox) | Library: schema parsing, validation and code generation. |
| [`avrox-cli`](crates/avrox-cli) | Command line frontend built on top of the library. |

## Usage

```sh
cargo run -p avrox-cli -- path/to/schema.avsc            # print to stdout
cargo run -p avrox-cli -- path/to/schema.avsc -o src/gen # write files
cargo run -p avrox-cli -- path/to/schema.avsc --layout module -o src/gen
cargo run -p avrox-cli -- --help
```

Options include `--language` (currently `rust`), `--avro-version`
(`1.10`, `1.11` or `1.12`, the default), `--layout` and `--types`:

- `--layout single` (default): everything in one file, named after the root
  type of the schema;
- `--layout module`: one file per named type plus a `mod.rs` declaring and
  re-exporting them; generated union enums live in `mod.rs`. The result can
  be used as a Rust module directory as is.
- `--types avro` (default): generated types rely on the official
  [`apache-avro`](https://docs.rs/apache-avro) crate — `bytes`/`fixed` use
  `serde_bytes`, and the `uuid`/`decimal`/`duration` logical types become
  `apache_avro::Uuid`/`Decimal`/`Duration`;
- `--types std`: only standard library types plus types generated into the
  output (a `Bytes` newtype and one newtype per `fixed` type, with `serde`
  implementations that serialize as Avro bytes). Logical types keep their
  underlying primitives. The generated code needs only `serde` to compile
  and still round-trips through `apache-avro`.

From Rust:

```rust
let files = avrox::generate(schema_json, &avrox::Settings::default())?;
```

## Generated Rust code

The generated code integrates with the [`apache-avro`](https://docs.rs/apache-avro)
crate through `serde`:

- records become structs, enums become enums;
- nullable unions (`["null", T]`) become `Option<T>`, other unions become
  generated enums with one newtype variant per branch, in branch order;
- recursive references in directly nested positions are boxed;
- logical types are documented on the fields and types carrying them;
- `bytes` and `fixed` representations depend on the `--types` mode (see
  above): `serde_bytes` types in `avro` mode, generated newtypes with
  byte-oriented `serde` implementations in `std` mode.

Both modes round-trip through `apache-avro`'s binary codec; `std` mode
requires only `serde` (with `derive`) as a direct dependency, `avro` mode
requires `serde`, `serde_bytes` and `apache-avro`.

Known limitations: schemas whose types map to colliding Rust names across
namespaces are rejected, Avro protocols (`.avpr`) are not supported, and
uuid logical types on `string` stay `String` in `avro` mode because
`apache_avro::Uuid` serializes as bytes in binary mode, which a string-based
uuid schema rejects.

## Development

Common commands are available as [`just`](https://just.systems) recipes — run
`just --list` to see them all:

```sh
just ci      # run the same checks as CI (fmt, clippy, build, test)
just test    # run all tests
just bless   # regenerate the expected outputs under tests/cases
just run path/to/schema.avsc  # run the CLI
```

Without `just`, the equivalent raw commands are:

```sh
cargo build --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all --check
```

## Releasing

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

Testing uses three layers:

- unit tests for the parser, naming and generator internals;
- black-box snapshot tests under `tests/cases`: every case is a directory with
  a `schema.avsc`, optional `settings.json`, and either an `expected/` directory
  with the expected generated files or an `error.txt` with the expected error;
- the snapshot expectations can be regenerated with `AVROX_BLESS=1 cargo test`.

## License

MIT
