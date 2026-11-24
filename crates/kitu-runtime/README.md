# kitu-runtime

Tick-based orchestrator that wires Kitu ECS, transports, and future scripting/timeline layers.

## Responsibilities
- Advance the simulation tick-by-tick and dispatch ECS systems deterministically.
- Run `update(dt)` with a fixed-timestep accumulator.
- Apply transport input on the next tick (`N` receive -> `N+1` apply).
- Emit staged runtime output after ECS dispatch and before transport polling.
- Implement the minimum player move vertical slice (`/input/move` -> `/render/player/transform`).
- Install persistent application hooks with validated, sequenced input metadata and detached projections.
- Bridge transports, scripting, and data playback while keeping the loop embeddable.

## Publish readiness
- Status: development preview (`0.1.0`).
- Run the full gate before toggling publication flags:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Runtime contract: [`doc/specs/runtime-execution-contract.md`](../../doc/specs/runtime-execution-contract.md)
- Runtime position within the workspace: [`doc/crates-overview.md`](../../doc/crates-overview.md)

## Application-selected spatial mode

The `rapier-physics` feature makes `kitu_runtime::physics` available. An
application can build both spatial modes into one binary and select one during
session initialization. Application callbacks use `RigidBodyWorld::advance`
at their fixed tick to update the integer obstacle scene. The
[spatial runtime contract](../../doc/specs/spatial-runtime.md) describes the
tick order and includes a runnable example.
