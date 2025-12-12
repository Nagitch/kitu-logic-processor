# kitu-scripting-rhai

Rhai scripting bindings and helpers for embedding scripts into the Kitu runtime.

## Responsibilities
- Provide a safe Rhai host configured for runtime data and APIs.
- Expose bindings that keep scripts decoupled from ECS internals while remaining deterministic.
- Offer helpers for loading, validating, and executing Rhai scripts during development.

## Publish readiness
- Status: internal (`publish = false`), but crates.io metadata is prepared alongside this README.
- Run the standard gate before flipping publication flags:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Workspace crate map: `doc/crates-overview.md`
