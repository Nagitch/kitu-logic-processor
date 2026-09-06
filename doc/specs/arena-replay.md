# Arena TSQ1 recording contract

Stage 7 of [the roadmap](https://github.com/Nagitch/kitu-logic-processor/issues/129)
uses the public `tsq1` and `tsq1-osc` APIs at workspace revision
`05707147e83a4fba591a3933d49f056bb6d3540b`. No TSQ1 format fork is involved.

## File and exact input ordering

The binary `.tsq` starts with `TSQ1`. A single track stores pairs:

1. Custom event type `0x4b`: JSON `{tick, order, metadata}`. Arena metadata contains
   the Runtime queue `sequence` and optional `{source, messageId, schemaVersion}`.
2. OSC MessagePack event: one immediate OSC-IR bundle. Every contained message is
   `[address, typeTags, arguments]`; tags `i`, `h`, `f`, `s`, `b` preserve i32, i64,
   f32, string and boolean identity. Argument/message ordering and empty bundles
   survive the public codec round trip. Unsupported/nested/scheduled bundles and
   non-finite or lossy f32 values are rejected, never flattened or coerced.

Exact `tick` and contiguous per-tick `order` determine application scheduling.
The 60 PPQ / 1,000,000 microseconds per quarter musical axis is a display mapping;
its deltas must agree with explicit ticks. No elapsed-time conversion or rounding
is used to reconstruct input ticks. Retransmissions remain in the input stream so
normal Runtime identity handling reproduces the original duplicate receipts.

The `KITU` extension chunk contains envelope format 1 and the Arena manifest.
Unknown tracks/chunks/axes are rejected by this application profile even though
TSQ1 itself supports them. The legacy `emit:`/`wait:` text helper remains available
for compatibility, but it is not a TSQ1 document and Arena does not use it.

## Session and version model

A recording starts at Runtime creation, before tick 0. It includes menu, pause,
empty-input and management ticks, all successful starts, next-run configuration
staging and retries. Starting recording halfway through a session is rejected;
there is no incomplete snapshot masquerading as an initial state.

The manifest stores:

- Arena recording/OSC contract versions, application ID and 60 Hz rate.
- Package version, execution hash and compilation target. The hash covers Arena
  rules, Runtime/ECS/core/OSC/transport sources, reference initial defaults, the
  locked dependency graph, compiler version and compilation flags. Unsupported
  builds fail compatibility checks. Keeping an old runtime binary is required
  when execution semantics change; a package version alone is insufficient.
- Full evaluated initial Tanu configuration, its semantic/source hashes and the
  evaluator revision. Every later staged configuration is retained in input data.
- Initial projection hash, completed tick count, a state and ordered-output hash
  for every tick, and each full run-start manifest with frozen evaluated values.

Replay builds the ordinary application with the detached initial content, feeds
its normal validated input queue, calls the same `tick_once`, and compares every
state/output before continuing. It never consults the current `.tmd` and never
sets HP, actors or clocks from recorded output. Altering the authoring file does
not affect old recordings. Divergence includes its exact tick in the diagnostic.
Hashes demand exact reproducibility within a compatible build; the independent
C# reference comparison continues to allow absolute float error `1e-4` while
requiring exact discrete state, hits/deaths and event ticks/order.

The first recorder is bounded to 216,000 management ticks (one hour), 1,000,000
input bundles and 64 MiB encoded files. Capture validates every OSC bundle and maintains a checked, conservative cumulative
encoded-size bound before appending a tick. Reaching a recorder bound reports an
incomplete-recording diagnostic without stopping live gameplay. Start a new host
session for a fresh recording. This is an explicit initial implementation limit,
not a claim of arbitrary-duration streaming or crash recovery.

## Host integration

All live ticks are captured before committed inputs are drained. Snapshot cloning
holds the state lock briefly; TSQ1 encoding, file I/O and replay verification run
outside the simulation lock. Recording files use SHA-256 content IDs and atomic
local rename. `KITU_ARENA_RECORDING_DIRECTORY` overrides the default
`apps/demo-game/.arena/recordings` directory.

| Endpoint | Result |
| --- | --- |
| `GET /arena/recording` | Recorded/live ticks, session ID, limits and diagnostics |
| `GET /arena/recording/export` | Download a complete detached `.tsq` |
| `POST /arena/recording/save` | Persist the current recording; return ID/path/size |
| `GET /arena/recordings` | List saved content IDs and sizes |
| `POST /arena/recordings/import` | Validate and save a binary TSQ1 request body |
| `GET /arena/recordings/{id}` | Download the saved binary |
| `POST /arena/recordings/{id}/verify` | Re-execute in a separate Runtime and return verified state |

File identifiers are restricted to SHA-256 hex strings. Import validates format,
versions, initial content and bounds; verify additionally executes all ticks and
checks proofs. Invalid files cannot mutate the live run. CPU verification is
serialized. These are local development endpoints under the existing host's
loopback default; production remote operation is outside the approved scope.

Admin playback, stop, step, seek and Unity replay projection are stage 8. Live
CLI commands are stage 9. Stage 7 supplies the real recorder, persistence and
re-execution core they will use.

## Reproduce the stock artifact

Inside the Kitu Dev Container:

```sh
KITU_REPLAY_EVIDENCE_DIR=/workspaces/kitu-logic-processor/.tmp/arena-tsq1 \
  cargo test -p kitu-demo-game --test arena_replay \
  real_tsq1_replays_every_stock_tick_state_and_event_through_eleven_death_retry -- --exact
```

The generated stock `.tsq` and verification JSON cover 5,528 ticks, 550 C#
checkpoints and 53 command outcomes, then compare every Rust state and ordered
output on replay. [Stage 7 evidence](../verification/arena-tsq1/results.json)
also records real macOS Unity input capture, HTTP save/download/import/verification,
and bounds. Generated recordings remain local; the test regenerates them for the
current execution version.
