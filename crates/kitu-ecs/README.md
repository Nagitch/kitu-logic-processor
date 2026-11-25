# kitu-ecs

Lightweight ECS world and scheduler powering the Kitu runtime loop.

## Responsibilities
- Track registered component types without locking the runtime into a heavyweight backend.
- Provide system scheduling hooks that keep ticking deterministic and testable.
- Own typed application resources with immutable/mutable access scoped to this world.
- Serve as the glue between runtime orchestration and domain-specific systems.

## Publish readiness
- Status: development preview (`0.1.0`).
- Run the standard gates before flipping publication flags:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Crate map and dependencies: `doc/crates-overview.md`

## Integer spatial queries

The `kitu_ecs::spatial` module provides checked millimetre placement and
micrometre positions, displacements, and axis-aligned bounds. Store a
`SpatialWorld` and the application's positions in typed ECS resources.
`move_aabb` sweeps a point or box to its first obstacle. `hit_segment`
returns the nearest collider and an exact fraction of the segment.
See the [spatial runtime contract](../../doc/specs/spatial-runtime.md).
