# kitu-data-sqlite

SQLite-backed data utilities and schema helpers for Kitu pipelines and runtime consumers.

## Responsibilities
- Encapsulate SQLite schema management, migrations, and query helpers shared across tools.
- Keep storage concerns isolated from runtime/timeline logic.
- Provide deterministic data access patterns suitable for headless or embedded hosts.

## Publish readiness
- Status: internal-only (`publish = false`), but crates.io-facing metadata and README are prepared.
- Execute the gate before changing publication flags:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Workspace crate overview: `doc/crates-overview.md`
