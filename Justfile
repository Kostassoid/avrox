set positional-arguments

# List available recipes.
help:
    @just --list

# Build all workspace targets.
build:
    cargo build --workspace --all-targets

# Run all tests: unit, snapshot and doc tests.
test:
    cargo test --workspace

# Check formatting without modifying files.
fmt:
    cargo fmt --all --check

# Apply rustfmt formatting to all files.
fix:
    cargo fmt --all

# Run clippy with the workspace lint configuration.
clippy:
    cargo clippy --workspace --all-targets

# Regenerate the expected outputs under tests/cases.
bless:
    AVROX_BLESS=1 cargo test -p avrox --test snapshots

# Run the avrox CLI with the given arguments.
run +ARGS:
    cargo run -p avrox-cli -- "$@"

# Run the same checks as CI.
ci: fmt clippy build test

# Remove build artifacts.
clean:
    cargo clean
