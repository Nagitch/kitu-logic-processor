# kitu-ecs

Lightweight ECS world and scheduler powering the Kitu runtime loop.

## Responsibilities
- Track registered component types without locking the runtime into a heavyweight backend.
- Provide system scheduling hooks that keep ticking deterministic and testable.
- Serve as the glue between runtime orchestration and domain-specific systems.

## Publish readiness
- Status: internal-only (`publish = false`), but metadata and README are ready for packaging once the MVP stabilizes.
- Run the standard gates before flipping publication flags:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Crate map and dependencies: `doc/crates-overview.md`
