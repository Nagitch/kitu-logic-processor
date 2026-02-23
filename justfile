# justfile — common development commands for kitu-logic-processor
# Run with: just <recipe>
# Install just: https://github.com/casey/just

# Format all code
fmt:
    cargo fmt --all

# Run Clippy lints (treat warnings as errors)
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Run all tests
test:
    cargo test --all

# Run fmt, lint, and tests in sequence
check-all: fmt lint test

# Build documentation
doc:
    cargo doc --no-deps --all-features

# Build the workspace (dev profile)
build:
    cargo build --all
