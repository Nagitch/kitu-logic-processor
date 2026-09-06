# Arena inspection

Stage 17, Issue #162. **Project → Arena Inspector** observes game state, entities,
the minimap, events, presentation cues and host timing from one verified Arena
snapshot. The standalone server and the embedded Player bridge expose the same
API. Inspection adds no game clock, controller, OSC field or recording field.

## Workflow

Open **Arena Inspector** with Admin connected to the host that owns the game.
Check its endpoint, session, Live/Replay mode, run and completed tick before
comparing values with Unity. Select an entity from the list or map to inspect
its authoritative position, health and other fields. The map uses Arena x/y
(Unity X/Z), with x right, y up and equal scale. Walls, collision radii, chest
and portal availability come from the application; the browser does not advance
or interpolate the game.

Use **Arena Replay** to save, import and load recordings. Inspector playback
controls operate that same host, then refresh the observed snapshot after the
owner applies the command. Step and seek can inspect a boss warning, transition,
death or retry at an exact tick. Returning to live restores the parked run;
resume the game explicitly. Source editing remains on **Game Parameters**,
**Game Scripts** and **Story Sequencing**, with the existing next-run adoption
rules. Inspector does not edit entities or adopt source changes.

The separate generic **World** and **Logs** pages are not sources for Arena
state or its tick. A failed refresh leaves the last snapshot visible and marked
stale; it must not appear to be a fresh observation.

## Read-only endpoint

`GET /arena/inspection` returns `InspectionSnapshot` and
`Cache-Control: no-store`. A successful UTF-8 JSON body is limited to 8,388,608
bytes during serialization. Unavailable, invalid or oversized observations
return a non-2xx JSON error envelope `{ "error": "bounded diagnostic" }`, never
a partial snapshot.

The owner captures all application facets from one
`GameState::application_projection()`, including the pre-tick state at tick -1.
During replay this is the last **verified** projection: an unsuccessful proof
may leave the replay Runtime ahead, so inspecting that Runtime directly would
misrepresent its state. The cache checks matching content/script/timeline/run
identities, matching state/presentation ticks and simulation counts, and equality
of full-timeline and compact presentation before publication.

GET clones the cache and current host mode under the existing owner mutex.
JSON encoding, statistics and network writes happen after releasing it. Reads
do not tick, enqueue input, claim or release a controller, read authoring files,
compile Rhai or decode TSQ1. Full source documents remain on the
[content](arena-content-sources.md), [script](arena-boss-scripts.md),
[timeline](arena-presentation-timelines.md) and [recording](arena-replay.md) APIs.

## Snapshot schema 1

All fields below are required; nullable fields use explicit JSON `null`.
`Hash` means 64 lowercase hexadecimal characters. `U64` and `I64` below are
canonical decimal **strings** in their Rust integer ranges: no whitespace,
plus sign, exponent, leading zero or `-0`. Completed tick fields are at least
`"-1"`. Do not convert wide strings to JavaScript `Number`.

```ts
type VersionHashes = { activeHash: Hash | null; pendingHash: Hash };
type InspectionSnapshot = {
  schemaVersion: 1;
  sessionId: string; // immutable host identity, at most 128 UTF-8 bytes
  revision: U64; attempt: U64; epoch: U64; run: U64;
  mode: {
    active: boolean; recordingId: Hash | null; tick: I64; totalTicks: U64;
    playing: boolean; seeking: boolean; error: string | null;
  };
  readOnly: boolean;
  execution: { package: string; sourceHash: Hash; target: string };
  versions: {
    content: VersionHashes; script: VersionHashes; timeline: VersionHashes;
  };
  diagnostics: {
    hostError: string | null; recordingError: string | null;
    scriptFault: InspectionScriptFault | null;
  };
  state: InspectionArenaState;
  presentation: InspectionPresentation;
  arena: {
    minX: -10; maxX: 10; minY: -10; maxY: 10; playerRadius: 0.5;
    chest: { x: -3; y: 0; interactionRadius: 2 };
    portal: { x: 0; y: 7.5; triggerRadius: 1.25 };
  };
  events: InspectionEventWindow;
  timing: InspectionTiming;
};
```

The initial revision, attempt, epoch and run are `"0"`. Live mode has a null
recording ID, totalTicks `"0"` and playing false. `mode.tick` equals the verified
`state.tick`. Queued replay activation may show active false, seeking true and
readOnly true while still displaying live state. `readOnly` follows the existing
replay/pending-replay input gate; it does not prevent playback commands.
An empty recording is a valid active replay at tick `"-1"` with totalTicks
`"0"`; it has no tick available for Play or Step.

`hostError` describes the latest failed owner attempt and clears after success.
`recordingError` retains the existing recorder failure. `scriptFault` belongs to
the same verified script snapshot. Diagnostics are capped at 4,096 UTF-8 bytes,
truncated at character boundaries with `… [truncated]`; existing smaller script
diagnostic limits still apply.

### Existing state and presentation types

`InspectionArenaState` preserves [ArenaState](../../apps/demo-game/src/arena/state.rs),
and `InspectionPresentation` preserves
[PresentationSnapshot](../../apps/demo-game/src/arena/presentation.rs), with only
these display-specific integer conversions:

| Object | Decimal string fields |
| --- | --- |
| `state` | `tick`: I64; `simulationSteps`: U64 |
| `presentation` | `tick`: I64; `run`, `simulationStep`: U64 |
| Each boss cue and non-null floor cue | `startedTick`: I64; `offsetTick`: U64 |
| `mode` | `tick`: I64; `totalTicks`: U64 |
| Non-null `diagnostics.scriptFault` | `tick`: I64 |

The remaining fault fields are `enemyId`, `scriptHash` and
`diagnostic: {kind, message, line, column}`; line/column are null or safe
nonnegative integers. Enemy, projectile, grenade and effect objects retain
their existing **PascalCase** fields, such as `Id`, `Position` and `Health`.
Item, Inventory, RunResult, Vec2 and top-level state keep their existing casing.
Entity/item IDs remain i32 numbers, cue event indices/counts remain u32 numbers,
and floating values must be finite. This adaptation does not change game OSC,
wire, replay or native state serialization.

### Publication and replacement identities

`revision` counts successful host publications, including cached paused-replay
publications. `attempt` counts actual owner-update calls after lock acquisition,
including failures. An unsuccessful call can advance attempt and diagnostics
without changing revision. A replay proof failure can publish status while
retaining all verified application facets. Neither counter means that gameplay
advanced.

`epoch` changes only on committed observation replacement: replay activation,
successful prepared seek (forward, backward, same tick or Stop to -1), or actual
return from replay to live. Queuing commands, failed or stale seek preparation,
play/pause/step, GET, reconnect and an already-live `live` command do not change
it. Cancelling pending activation while still observing the same live Runtime
also leaves epoch unchanged. Starting a run changes run, not epoch.

Selection identity is `(sessionId, epoch, run, entityKind, id)`. Clear selection
on a context change or disappearance, since IDs may be reused by another run.
Observation counters use checked arithmetic; exhaustion makes inspection
explicitly unavailable until a new session, without wrapping identities or
stopping otherwise-valid gameplay.

## Retained events

```ts
type InspectionEventWindow = {
  capacity: 256; byteLimit: 2097152; payloadByteLimit: 65536;
  firstSequence: U64 | null; lastSequence: U64 | null;
  dropped: U64; omittedPayloads: U64; entries: InspectionEvent[];
};
type InspectionEvent = {
  sequence: U64; revision: U64; epoch: U64; run: U64; tick: I64;
  bundleIndex: number; messageIndex: number; // original u32 batch positions
  address: string;
  detail:
    | { kind: "message"; messageJson: string }
    | { kind: "run"; contentHash: Hash; scriptHash: Hash; timelineHash: Hash }
    | { kind: "oversize"; encodedBytes: U64; sha256: Hash };
};
```

Capture `/game/arena/*`, `/ui/arena/command`, `/ui/arena/use` and
`/ui/arena/timeline/event` from successful live ticks or verified replay steps,
in original bundle/message order. Repeated state/render snapshots, paused-replay
republication, GET, activation and seek replacement create no history entries.
Seek fast-forward does not import its intermediate events into this window.
This is **events observed since this snapshot**, not a recording's full history.

Sequences start at `"0"` and never reset. Epoch replacement clears retained
events and eviction/omission counters; an empty window has null sequence
endpoints. All runs within the epoch can remain in the window; Inspector labels
its default current-run filter. `/game/arena/run` changes attribution for that
and subsequent messages in the tick. A timeline audit's explicit run overrides
attribution for that message alone, preserving late stops from an older run.

Run events are summaries containing the three adopted version hashes. Other
events carry canonical shared `WireMessage` JSON **inside a string**, preserving
typed OSC values and wide input IDs through outer JSON parsing. Display this as
escaped text. Optional entity selection must validate a parsed ID as i32 rather
than rounding an arbitrary integer.

A message exceeding 65,536 canonical bytes becomes an explicit byte-count/SHA-256
marker computed over its complete bytes, not truncated JSON. Retention counts
complete compact event JSON, including escaped message strings, and evicts oldest
entries until both 256 entries and 2 MiB hold. `dropped` counts actual evictions;
`omittedPayloads` counts oversize markers since the epoch began. These diagnostics
do not modify game output or recording proofs.

## Host owner-update timing

```ts
type InspectionTiming = {
  scope: "hostUpdate"; capacity: 256; budgetUs: 16666.666666666668;
  maxDurationUs: 86400000000;
  totalSamples: U64; overBudgetSamples: U64;
  last: InspectionTimingSample | null; samples: InspectionTimingSample[];
  meanUs: number | null; p95Us: number | null; maxUs: number | null;
};
type InspectionTimingSample = {
  attempt: U64; revision: U64; tick: I64;
  outcome: "idle" | "advanced" | "replacement" | "fault";
  runtimeAdvanced: boolean; simulationAdvanced: boolean;
  durationUs: number; lockWaitUs: number;
  durationClamped: boolean; lockWaitClamped: boolean; overBudget: boolean;
};
```

`lockWaitUs` measures owner-mutex wait. `durationUs` measures work after acquiring
it, including verified projection/event capture and existing in-lock recorder
and host bookkeeping. GET, statistics, JSON/network writes, asynchronous
persistence and separately prepared seek work are excluded. The display is
**host owner-update cost**, not FPS, pure gameplay CPU cost or game time.

Every owner attempt contributes a sample. `advanced` means a verified selected
Runtime tick, `idle` means none, `replacement` means an observed projection
replacement, and `fault` means an owner/verification/capture failure. Load/seek
replacements have both advancement flags false, even when the destination tick
is greater. Return-live can also perform the existing live tick: its flags and
events report that actual work after resetting the observation context.

Paused live ticks advance Runtime but not simulation; paused replay advances
neither. Failed proof samples use the last verified tick. A late Rhai fault can
still be a successful management tick with Runtime advancement and no game step;
its script fault is reported separately.

The 256-sample window and totals reset on epoch or run change, including that
call's new-context sample. Attempt never resets. Statistics cover retained
samples only: mean, maximum and nearest-rank p95 (`ceil(0.95 * count) - 1` in
sorted order). Before any sample, last/statistics are null and totals are `"0"`.
Microseconds are finite and nonnegative, with a disclosed 24-hour display cap;
statistics involving capped values are lower bounds. `overBudget` compares the
original elapsed duration with exactly 1/60 second before rounding or capping.
Timing is host-only and never enters deterministic state, OSC or TSQ1 proofs.

## Browser freshness and exact seek

Inspector polls every 200 ms while visible, with one request in flight. Endpoint
changes and unmount abort the request and invalidate its generation; returning
to visibility refreshes immediately. A response is validated as one immutable
snapshot before any panel changes. Failure retains a visibly stale previous
snapshot. Within a session, lower revision, attempt or epoch is stale; equal
identities can carry newer queued mode status and remain admissible. A new
session resets this ordering. Tick order alone cannot identify freshness because
successful backward seek is valid.

Seek reuses `POST /arena/playback/seek` with numeric JSON `{ "tick": i64 }`.
The Inspector accepts exact canonical text `-1` or a nonnegative integer up to
9223372036854775807 and below `mode.totalTicks`. It validates the digits before
placing them directly in the JSON number token, without a JavaScript Number
conversion. HTTP command replies are acknowledgments; only a refreshed snapshot
confirms the owner-applied result. Rejected seeks preserve the observed state.
Confirmation remains bound to the host session and, except when returning to
live, the requested recording. Another recording at the same destination tick
cannot confirm the operation. Play may reach the final verified tick before the
next poll; actual advancement to that endpoint confirms completion even though
playing is already false. An unchanged, already-ended replay does not count as
advancement.

## Validation scope

Stage verification must cover initial/live/paused/replay snapshots, forward and
backward replacement, retry with reused IDs, failed replay retaining all verified
facets, failed owner attempts, event order and bounds, decimal limits, and reads
that leave controller ownership and deterministic outputs unchanged. Frontend
checks must cover coherent adoption, stale/aborted responses, exact seek,
selection and map coordinates. Actual server and embedded Player observations
must compare all panels against their own owner; performance assertions concern
scope and valid bounds, not exact elapsed values.
