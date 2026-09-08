# kitu-data-tmd

Parser and loader utilities for Tanu Markdown (TMD) data used by the Kitu runtime.

## Responsibilities
- Parse authored TMD assets into strongly typed structures.
- Keep schema evolution and validation logic isolated from runtime code.
- Prepare data for deterministic playback pipelines.

## Real Tanu documents

`tables::TanuDocument` reads/writes the actual ZIP container and delegates named
table evaluation to Tanu's public `tmd-core` API, pinned at
`194358e8791f1391492abcb60d8cfcc37bbb383a`. Formula and container semantics belong
to Tanu; applications validate the returned typed rows and decide when to activate
them. Read/evaluate off the simulation tick, then queue detached values.

`sources` / `set_sources` expose typed managed Formula definitions for authoring.
`edit` delegates Tanu's cell write-back API for supported Formula-query sources;
managed tables use source definitions instead. Definitions can retain diagnostic
drafts, so evaluate before activating or presenting a candidate as valid.

The top-level line-oriented key/value helper remains for compatibility with early
fixtures. It is not a TMD container parser. Applications own their TMD authoring,
validation, and activation workflow.

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
