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

# Print unreleased changes (commits since the last tag) to paste into
# CHANGELOG.md's "Unreleased" section.
changelog:
    git-cliff --config cliff.toml --unreleased

# Cut a release: bump all versions, rotate the changelog, tag and push.
# Pass a version (e.g. `just release 1.1.0`); add `--execute` to perform it
# (without it cargo-release runs as a dry run).
release +ARGS:
    cargo release "$@"

# Regenerate the example models and run the example application as a smoke test.
smoke:
    cargo run -p avrox-cli -- generate examples/user-profile.avsc --output-file examples/rust/src/generated.rs
    cargo run --manifest-path examples/rust/Cargo.toml

# Run the same checks as CI.
ci: fmt clippy build test smoke

# Remove build artifacts.
clean:
    cargo clean
