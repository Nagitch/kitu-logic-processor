# kitu-tsq1

TSQ1 timeline AST and playback helpers for Kitu presentation flows.

## Responsibilities
- Model TSQ1 timelines and events in a deterministic, testable form.
- Provide helpers for driving playback that emit OSC/IR messages.
- Remain decoupled from transport specifics so multiple frontends can reuse the logic.

## Publish readiness
- Status: internal (`publish = false`), but crates.io metadata and README are ready for packaging.
- Run the publication gate before enabling release:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Timeline positioning within the workspace: `doc/crates-overview.md`
