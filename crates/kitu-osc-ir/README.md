# kitu-osc-ir

OSC-inspired intermediate representation for messages flowing through Kitu transports and runtime APIs.

## Responsibilities
- Define the message shapes exchanged between transports and higher-level runtime logic.
- Stay transport-agnostic so tooling and backends can interoperate safely.
- Provide a stable surface area that downstream crates can depend on without heavy dependencies.

## Publish readiness
- Status: internal (`publish = false`), but metadata and README are ready for crates.io when publishing is allowed.
- Execute the standard quality gates before changing publication settings:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Crate map and data-flow notes: `doc/crates-overview.md`
