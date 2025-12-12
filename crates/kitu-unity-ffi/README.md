# kitu-unity-ffi

C ABI bindings that expose the Kitu runtime to Unity hosts.

## Responsibilities
- Marshal data between Unity buffers and the Rust runtime API.
- Provide a stable ABI surface appropriate for a `cdylib` target.
- Keep Unity-facing details isolated from core runtime logic.

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
