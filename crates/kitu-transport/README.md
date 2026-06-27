# kitu-transport

Transport abstraction and adapters for moving OSC/IR messages between the runtime and external systems.

## Responsibilities
- Define the `Transport` trait for sending and receiving OSC/IR envelopes.
- Provide composable adapters (in-memory, network, etc.) without dictating routing policies.
- Keep the runtime deterministic by clearly separating transport concerns from game logic.

## KEP and OSC helpers

`kitu-transport` provides MessagePack KEP helpers:

- `KepEnvelope`
- `encode_kep_envelope`
- `decode_kep_envelope`

It also provides OSC packet helpers for the OSC-IR model:

- `encode_osc_packet`
- `decode_osc_packet`
- `encode_osc_bundle`
- `decode_osc_bundle`

Supported OSC message argument types:

- `i`: int32
- `h`: int64
- `f`: float32
- `s`: string
- `T` / `F`: bool

Supported OSC packet shapes:

- Single OSC messages.
- OSC bundles containing message elements with the immediate timetag.

Nested OSC bundles, blobs, arrays, and additional scalar tags remain unsupported
until a concrete Kitu client or runtime path requires them.

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
