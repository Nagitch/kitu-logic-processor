# kitu-runtime

Tick-based orchestrator that wires Kitu ECS, transports, and future scripting/timeline layers.

## Responsibilities
- Advance the simulation tick-by-tick and dispatch ECS systems deterministically.
- Bridge transports, scripting, and data playback while keeping the loop embeddable.
- Expose configuration hooks (tick rate, logging) that callers can tune per host.

## Publish readiness
- Status: internal (`publish = false`) but documentation and metadata follow the crates.io guidelines for later publication.
- Run the full gate before toggling publication flags:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Runtime position within the workspace: `doc/crates-overview.md`
