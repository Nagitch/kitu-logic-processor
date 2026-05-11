# kitu-unity-ffi

C ABI bindings that expose the Kitu runtime to Unity hosts.

## Responsibilities
- Marshal data between Unity buffers and the Rust runtime API.
- Provide a stable ABI surface appropriate for a `cdylib` target.
- Keep Unity-facing details isolated from core runtime logic.

## Current MVP surface
- `kitu_init` creates a runtime handle for an embedding host.
- `kitu_submit_move_input` submits one `/input/move` intent into runtime-owned processing.
- `kitu_tick` advances the runtime by one authoritative tick.
- `kitu_pop_render_transform` drains one `/render/player/transform` event for presentation consumers.

The crate intentionally keeps gameplay rules inside `kitu-runtime`; this boundary only translates host calls into runtime input/output.

## Publish readiness
- Status: internal (`publish = false`), but metadata and README prepare the crate for crates.io packaging.
- Run the gate before enabling publication:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Workspace crate overview: `doc/crates-overview.md`
