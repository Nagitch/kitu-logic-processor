# kitu-tsq1

Real TSQ1 binary recordings, typed OSC-IR conversion and legacy timeline helpers.

`presentation::Clip` encodes and decodes bounded real TSQ1 presentation clips.
Each `ScheduledEvent` retains its exact integer offset, original track/event
indices, and ordered flat OSC bundle. Merged order is
`(offset_tick, track_index, event_index)`, including simultaneous events. The
application owns the clock, cursor and effects; preparation belongs outside its
simulation lock. Empty tracks and generic clips are preserved.

Presentation uses nonzero PPQ equal to its tick rate and one-second quarters.
There is no float-time conversion. Absolute events, varying tempos, markers,
sync/SMPTE, unknown chunks/flags, non-OSC events, raw/CBOR OSC, scheduled/nested
bundles and lossy scalar conversions are rejected. Applications further validate
their own rate, addresses, ranges and required initialization/terminal events.

Clips are limited to 8 KiB, 8 tracks, 256 events and offsets through tick 3600.
Each bundle allows 32 messages, each message 16 scalar arguments, addresses 256
UTF-8 bytes and strings 1024 bytes. Addresses start with `/`; NUL and nonfinite
floats are rejected. Encoding validates these limits before cloning data and
checks the final encoded byte count. Decoding checks the input size first, then
preflights MessagePack with depth 32 and exact byte consumption before allocating
an OSC-IR tree. Malformed declared lengths and trailing data are errors. Work is
bounded by the clip limits; there is no background thread, file I/O or clock.

`Clip::encode` writes canonical TSQ1 framing. Decoding and encoding preserve
logical events, not arbitrary original framing; applications retain and hash
original source bytes when those identify a content version. The pinned TSQ1
header's track count is advisory: actual decoded tracks determine `track_count`.

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
