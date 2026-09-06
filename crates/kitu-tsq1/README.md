# kitu-tsq1

Real TSQ1 binary recordings, typed OSC-IR conversion and legacy timeline helpers.

`recording::Recording` uses the pinned public TSQ1/OSC APIs, with explicit tick/order
envelopes and application-owned manifests. i32/i64 widths, finite f32 values,
argument/message ordering and immediate bundles round trip losslessly. Unsupported
bundle semantics are rejected. See `doc/specs/arena-replay.md` in the repository.

The old `Timeline::parse` emit/wait text helper remains a compatibility API; it is
not the binary TSQ1 format and is not used by Arena recording.

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
