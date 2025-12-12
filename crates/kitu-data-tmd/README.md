# kitu-data-tmd

Parser and loader utilities for Tanu Markdown (TMD) data used by the Kitu runtime.

## Responsibilities
- Parse authored TMD assets into strongly typed structures.
- Keep schema evolution and validation logic isolated from runtime code.
- Prepare data for deterministic playback pipelines.

## Publish readiness
- Status: internal (`publish = false`), with crates.io metadata and README in place for future packaging.
- Run the workspace gates before enabling publication:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Workspace crate overview: `doc/crates-overview.md`
