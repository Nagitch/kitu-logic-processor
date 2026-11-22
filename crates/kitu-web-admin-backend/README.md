# kitu-web-admin-backend

Backend glue (HTTP/WS) for the Kitu web admin experience.

## Responsibilities
- Provide HTTP/WS endpoints that surface runtime diagnostics and controls.
- Delegate simulation work to runtime crates while keeping web concerns isolated.
- Support live inspection hooks needed by the admin frontend.

## Publish readiness
- Status: development preview (`0.1.0`).
- Run the quality gate before enabling publication:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Workspace crate overview: `doc/crates-overview.md`
