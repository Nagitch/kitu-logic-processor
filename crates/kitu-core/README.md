# kitu-core

Foundational Kitu runtime primitives (errors, ticks, timestamps, and shared utilities) used across the workspace crates.

## Responsibilities
- Provide the shared `KituError` type and `Result` alias so downstream crates can stay dependency-light.
- Keep tick and timestamp helpers consistent between the runtime loop and supporting tooling.
- Host small, reusable utilities that should not pull in heavier dependencies.

## Publish readiness
- Status: development preview (`0.1.0`).
- Before toggling publication, run the workspace quality gates:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Workspace map and crate relationships: `doc/crates-overview.md`
