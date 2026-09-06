# Arena TSQ1 presentation timelines

Stage 14, Issue #156. Arena uses real TSQ1 clips for boss warnings and floor
transitions. Game rules still own boss phases, movement, damage and progression.
The frozen Unity-only game projection and domain-event ordering remain the
comparison oracle.

## Edit and apply

The fixed authoring files are `boss-telegraph.tsq` and `floor-transition.tsq`.
The server accepts their directory through `KITU_ARENA_TIMELINE_DIRECTORY`.
Without that option, validation uses the two bundled files in
`apps/demo-game/content/timelines/`.

Create or edit a sample using the application's public-codec authoring tool in
the Dev Container:

```sh
cargo run --locked -p kitu-demo-game --bin arena-timelines -- /tmp/arena-clips
cargo run --locked -p kitu-demo-game --bin arena-timelines -- /tmp/arena-clips --boss-radius 4.5 --floor-peak-opacity 0.85
```

Then use **Admin → Story Sequencing → Reload and validate**, inspect the candidate
hash/files, and stage it for the next run. The shared terminal and browser Shell
commands are:

```text
inspect timeline
timeline validate
timeline stage <candidate-hash>
```

`GET /arena/timeline` returns the source directory, candidate, diagnostics, active
and pending versions, and the observed run's complete presentation. Validation
uses `POST /arena/timeline/validate`; staging uses
`POST /arena/timeline/stage` with `{ "hash": "..." }`. A stale or invalid candidate
is refused. Applying an edit never replaces the current run's adopted clips.
The next successful start/retry adopts the pending version. Invalid files retain
the last valid active/pending versions and clear the invalid candidate.

Native configuration accepts detached `timeline` and optional absolute
`timelineDirectory`. Unity accepts `--arena-timeline /absolute/directory`.
With embedded storage, two editable default files are seeded in `timelines/`
only when absent. A caller's external directory is not seeded. Detached factory
content is never replaced by authoring files; `{}` uses bundled data without
authoring I/O or a listener.

## Clip and OSC contract

The shared `kitu-tsq1::presentation::Clip` API uses the pinned TSQ1 and OSC public
APIs. Musical deltas are exact integers: 60 PPQ, one tempo entry at tick 0 with
1,000,000 microseconds per quarter, no other timing axes or tempo changes. Events
are ordered by `(offsetTick, trackIndex, eventIndex)`, including equal offsets.
Original ordered message/argument boundaries and supported scalar types survive
the codec round trip. Arena does not use absolute-time conversion or rounding.

Each clip is at most 8 KiB, eight tracks, 256 events and offset 3600. The decoder
checks input bytes before allocating a TSQ1 model and preflights MessagePack with
depth 32 and exact consumption before OSC conversion. Unsupported chunks, timing,
event formats, nested/scheduled bundles and lossy/nonfinite floats are rejected.

| Clip | Allowed message | Exact argument types and ranges |
| --- | --- | --- |
| `boss-telegraph` | `/render/arena/cue/boss` | f32 radius 0.25–8 world units; f32 intensity 0–1 |
| `floor-transition` | `/render/arena/cue/floor` | f32 opacity 0–1 |

Every clip initializes its values at offset 0. The floor clip's last effective
assignment is opacity 0. Clips contain presentation assignments only. Their
messages cannot change gameplay, invoke arbitrary commands or start another clock.

## Authoritative clock and display

After a successful gameplay update, Arena removes ended boss cues, advances
remaining cues once, applies due TSQ1 events, then creates newly triggered cues at
offset 0. An actual boss phase 0→1 starts a warning; death, removal or leaving
telegraph ends it. The last authored warning remains until the real phase ends,
including a longer Rhai telegraph. Entering floor transition starts a floor cue;
it can finish after the next floor begins. Its terminal event removes the cue.
Menu, results and retry clear presentation instances. Cues are uniquely identified
within each run and boss snapshots sort by entity ID.

Pause, UI overlays, disconnect and Rhai faults freeze cue offsets and values.
Management ticks continue. Transition updates count as gameplay steps even though
the reference game's elapsed-time field intentionally does not advance there.

New presentation events are emitted after the original game events, preserving
their original order fields. `/ui/arena/timeline/event` identifies the run, tick,
order, cue, clip, offset and start/event/stop operation. Applied events retain the
track/event indices and exact typed OSC bundle. `/ui/arena/timeline` publishes
detached versions on adoption/staging/inspection.

`/render/arena/presentation` is a compact complete snapshot after `/ui/arena/state`:
contract version, run, management tick, simulation step, boss cues and optional
floor cue. Each cue carries its source ID, start/offset tick and consumed/total
event count. Boss cues carry entity/floor/radius/intensity; floor cues carry
from/to floor and opacity. Unity draws these values directly, with the floor
fade behind the HUD. The client buffers split WebSocket messages and publishes
state and presentation together only when both management tick and simulation
step match. Reconnect discards incomplete pairs; backward seeks remain valid.
Rendering and GUI frames never advance a cue. Admin shows
the same observed run and exact positions, including paused replay and seek.

## Versioning and replay

`TimelineVersion` contains contract version 1, tick rate 60 and the two ordered
clip IDs, source SHA-256 hashes and exact bytes. Its hash binds all identities.
Encoded versions are at most 128 KiB. File reads and clip preparation run outside
the clock lock. Catalogs, sessions and replay retain immutable decoded handles;
ordinary queue validation uses a bounded admission pool without parsing on tick.

Native callers use `host:arena-timeline`; Admin/Shell exclusively own
`host:arena-timeline-admin` and its counter. External ingress cannot impersonate
the operator identity. Accepted/rejected/duplicate receipts follow the usual
Runtime queue and retain original IDs.

Recording manifest 3 requires `initialTimeline`. Run manifests and staged inputs
retain detached versions; recorder and decoder both allow at most 64 distinct
versions per recording. Saved files replay through the same queue, tick, output
and projection checks after authoring changes or deletion. Step and seek restore
complete presentation state. The execution fingerprint includes the shared TSQ1
implementation and bundled clips. Earlier recordings use retained earlier
executables; version checks are never bypassed or rewritten.

Verification covers real binary editing, invalid/stale/next-run activation,
gameplay parity, exact native/server output, graphical cues, pause, and detached
replay/seek. Stage evidence is recorded before the stage PR is merged.
