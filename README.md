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
cargo run -p avrox-cli -- --help
```

Options include `--language` (currently `rust`) and `--avro-version`
(`1.10`, `1.11` or `1.12`, the default).

From Rust:

```rust
let files = avrox::generate(schema_json, &avrox::Settings::default())?;
```

## Generated Rust code

The generated code integrates with the [`apache-avro`](https://docs.rs/apache-avro)
crate through `serde`:

- records become structs, enums become enums, `fixed` become
  `serde_bytes::ByteArray` aliases and `bytes` becomes `serde_bytes::ByteBuf`;
- nullable unions (`["null", T]`) become `Option<T>`, other unions become
  generated enums with one newtype variant per branch, in branch order;
- recursive references in directly nested positions are boxed;
- logical types keep their underlying primitive representation and are
  documented on the fields and aliases carrying them.

The consuming crate needs `serde` (with `derive`) and `serde_bytes` as
dependencies, and `apache-avro` to (de)serialize.

Known limitations: schemas whose types map to colliding Rust names across
namespaces are rejected, and Avro protocols (`.avpr`) are not supported.

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

Testing uses three layers:

- unit tests for the parser, naming and generator internals;
- black-box snapshot tests under `tests/cases`: every case is a directory with
  a `schema.avsc`, optional `settings.json`, and either an `expected/` directory
  with the expected generated files or an `error.txt` with the expected error;
- the snapshot expectations can be regenerated with `AVROX_BLESS=1 cargo test`.

## License

MIT
