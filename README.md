# avrox

Avro code generator.

## Workspace

| Crate | Description |
| --- | --- |
| [`avrox`](crates/avrox) | Library that turns Avro schemas into source code. |
| [`avrox-cli`](crates/avrox-cli) | Command line frontend built on top of the library. |

## Usage

```sh
cargo run -p avrox-cli -- path/to/schema.avsc
```

## Development

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all --check
```

## License

MIT
