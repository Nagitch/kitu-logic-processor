# kitu-transport

Transport abstraction and adapters for moving OSC/IR messages between the runtime and external systems.

## Responsibilities
- Define the `Transport` trait for sending and receiving OSC/IR envelopes.
- Provide composable adapters (in-memory, network, etc.) without dictating routing policies.
- Keep the runtime deterministic by clearly separating transport concerns from game logic.

## Publish readiness
- Status: internal-only (`publish = false`) while the MVP takes shape; metadata now aligns with crates.io requirements.
- Before enabling publication, run the workspace gates:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Transport positioning within the runtime: `doc/crates-overview.md`
