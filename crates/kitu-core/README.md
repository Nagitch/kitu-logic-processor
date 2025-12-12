# kitu-core

Foundational Kitu runtime primitives (errors, ticks, timestamps, and shared utilities) used across the workspace crates.

## Responsibilities
- Provide the shared `KituError` type and `Result` alias so downstream crates can stay dependency-light.
- Keep tick and timestamp helpers consistent between the runtime loop and supporting tooling.
- Host small, reusable utilities that should not pull in heavier dependencies.

## Publish readiness
- Status: internal-only for now (`publish = false`), but metadata is filled so the crate can be packaged when the MVP is ready.
- Before toggling publication, run the workspace quality gates:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Workspace map and crate relationships: `doc/crates-overview.md`
