# kitu-shell

CLI shell primitives for driving and inspecting the Kitu runtime during development.

The shared live catalog, command quoting, typed OSC conversion and wire contracts
are used by `kitu-cli` and the Admin browser Shell. Commands execute against the
connected host; application actions/scenarios stay application-owned. See the
[live command contract](../../doc/specs/live-shell.md) for usage, outcomes,
idempotent retry and initial bounds. The legacy in-process registry remains
available for embedders.

## Responsibilities
- Offer developer-focused commands for diagnostics, replay, and scripting entry points.
- Stay thin, delegating core logic to runtime crates while providing ergonomic wrappers.
- Enable headless testing workflows that mirror embedded host behavior.

## Publish readiness
- Status: internal (`publish = false`), but crates.io metadata and README are staged for future publication.
- Execute the publication gate before enabling releases:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Workspace crate overview: `doc/crates-overview.md`
