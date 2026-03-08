# kitu-runtime

Tick-based orchestrator that wires Kitu ECS, transports, and future scripting/timeline layers.

## Responsibilities
- Advance the simulation tick-by-tick and dispatch ECS systems deterministically.
- Run `update(dt)` with a fixed-timestep accumulator.
- Apply transport input on the next tick (`N` receive -> `N+1` apply).
- Emit staged runtime output after ECS dispatch and before transport polling.
- Bridge transports, scripting, and data playback while keeping the loop embeddable.

## Publish readiness
- Status: internal (`publish = false`) but documentation and metadata follow the crates.io guidelines for later publication.
- Run the full gate before toggling publication flags:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Runtime contract: `specs/runtime-execution-contract.md`
- Runtime position within the workspace: `doc/crates-overview.md`
