# Stage 17: coherent Arena inspection

[Issue 162](https://github.com/Nagitch/kitu-logic-processor/issues/162).
The [inspection contract](../../specs/arena-inspection.md) defines the shared
server/embedded endpoint, exact integer representation, retained event window
and host owner-update timing. [results.json](results.json) identifies completed
checks and remaining work. No PR or merge completion is claimed here.

**Pending:** PR/CI/review/merge and the parent workspace reference update.
All local checks listed below are complete.

| Verification | Recorded result |
| --- | --- |
| Dev Container workspace | 360 tests/doctests across 58 suites; zero failures or ignored cases; formatting, all-target/all-feature Clippy and warnings-denied rustdoc passed |
| Scoped backend | 43 host tests, including 13 inspection cases; 29 transport tests and 20 app/transport doctests passed |
| Frontend | 19 Inspector tests; Svelte check and lint passed; full WASM/Vite build and final Inspector Vite build passed |
| Unity 6000.6.0f1 | 143 EditMode, all 31 MessagePack PlayMode and seven JSON network PlayMode cases passed; zero failures or skipped cases |
| Native package | macOS ARM64 library built; nine C ABI exports, relocatable install name and signature verified |
| Player build | Signed macOS ARM64 app contains the verified native library, default source package and actual local Addressables catalog/bundle |
| Stock recording preparation | Server and native execution each verified 5,528 ticks / 5,581 inputs, including 11F death and retry |
| Actual graphical Player | 5,528 ticks / 5,581 inputs match every complete logical state/output; death/retry and five rendered checkpoints observed |
| Server and embedded Player Admin browsers | Both selected-host flows passed complete displayed-state, map/selection, cue, replay, stale/reconnect, 390px layout and navigation checks |
| Retained executions | Linux server, raw native dynamic/static libraries and the signed Player archived locally with hashes |

## Observation and replay checks

[unity-tests.json](unity-tests.json) retains source-report/XML hashes, suite
counts and compact Inspector checkpoints. The native test uses a manual owner:
repeated HTTP GETs leave the complete initial snapshot, native output and tick
unchanged. Start advances Runtime and simulation; pause advances management tick
while holding game time. Neither GET nor the observer creates a second owner.

Both network encodings compare complete typed Unity state and presentation with
the inspection endpoint at replay initial state, floor cue tick 197, boss cue
1649, 11F death 5526, retry 5527, backward seek 197, step 198 and Stop to -1.
The floor offset is 12 at tick 197; the boss offset is 24 at tick 1649. Player
transforms and the rendered boss ring match the projected values within the
existing `1e-4` transform tolerance. Successful seeks change observation epoch;
step retains it. Paused replay and repeated inspection do not duplicate events.
The fixture returns to the parked paused live run.

These are typed C# comparisons after explicit parsing of inspection-only decimal
strings. Raw `JsonUtility` evidence is not claimed to be byte-identical to API
JSON; the compact records retain the actual API state/presentation digests and
source-report hashes without publishing full snapshots.

[rust.json](rust.json) distinguishes scoped checks from the workspace run.
Workspace tests used `cargo test --locked --workspace`; rustdoc used
`cargo doc --locked --workspace --no-deps`. Clippy included all targets/features;
the test and rustdoc commands did not add `--all-features`.
Focused host cases cover verified-state retention after replay proof failure,
failed attempts without a new publication, actual return-live timing, mixed runs
and late old-run timeline events, count/byte eviction, oversize payload hashes,
exact wide integers and observation failures that preserve successful game
outputs. Timing statistics are checked by scope and bounds; measured durations
are not deterministic gameplay assertions.

[frontend.json](frontend.json) records atomic DTO adoption, one request in flight,
late-response generation guards, hidden/unmounted cleanup, stale-state retention,
exact seek JSON, map coordinates and run-scoped entity selection. Final cases
also cover an empty recording at -1, Play reaching EOF before the next poll and
a competing recording that must not confirm another recording's seek or Stop.
The final Inspector-only corrections followed the full WASM build and were
rechecked with tests, type/lint checks and Vite; no Rust/WASM source changed.

## Build identities and local browser proof

[builds.json](builds.json) binds the native package and Player build reports.
The signed plugin SHA-256 is
`0e684dd67200b2f52c279e7ffeb5ca6f29ae64403a599936e0c3fac1826ad9ca`;
the same bytes are in the app. Its install name is
`@rpath/libkitu_demo_game_native.dylib`. The Player contains default package
`0e003376f7788866f6484137bedbfe2ef62c7c8b7d9f7ac61994eeb0c59406e5`
and an Addressables 2.11.2 local catalog with one 259,644-byte bundle. The build
reported zero errors and four warnings; it is a local development build, with
no cloud symbol upload or Pipeline runtime configuration required for Arena.

Server and native reports retain their own execution identity and recording
SHA-256. They are verified separately with the corresponding build; recording
identities are not rewritten to make unlike executions interchangeable.

[player.json](player.json) records the actual graphical standalone's stock run,
using its native backend and the preserved trace/expected output. The independent
verifier compares every complete logical state and ordered output. All five
required images exist; no exact-pixel-equality claim is made. Trace, expected,
actual and binary hashes retain the provenance even when raw JSON serialization
differs.

[browser.json](browser.json) contains completed flows for the server at port 8795
and the relocated Player's embedded bridge at port 8796. Each browser compares
its complete displayed state to its selected API and checks entity selection/reset, floor and boss cue
positions, forward/backward seek, step, Play to EOF, pause, Stop and return-live.
An aborted inspection request marks the unchanged state stale; reconnection
recovers. The 390px layout and navigation/remount checks pass with no page errors.
The initial harness compared objects reserialized by the browser automation
transport, which altered three numbers by approximately `1e-16`. Reading the DOM's
JSON text preserves the exact values; the final flow passes without production
changes or a relaxed comparison tolerance.

[runtime-archive.json](runtime-archive.json) identifies the retained Linux server,
macOS native dynamic/static libraries and signed Player, including the Player
file-inventory digest. Capture occurred with implementation changes in the
worktree; the report records that fact rather than treating the base revision
as a clean build revision. The two execution identities remain in the build and
observation reports for subsequent recording compatibility checks.

Large DOM/state reports, recordings, binaries and PNGs remain ignored local
artifacts. No screenshots are uploaded for this stage.

## Reproduce

Run Rust/frontend checks in the Dev Container using the recorded command scopes:

```sh
cargo fmt --all -- --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps
cd tools/kitu-web-admin/frontend
pnpm test:inspection
pnpm check
pnpm lint
pnpm build
```

Use the [macOS build instructions](../../../kitu-integration-runner/unity-demo-game/README.md#reproduce-the-embedded-macos-build)
to package the native library and build the Player. Unity requires the licensed
6000.6.0f1 Editor and macOS SDK. For PlayMode, run an isolated server, generate
and verify a stock recording with that execution, and supply
`KITU_ARENA_WS_URL`, `KITU_ARENA_REPLAY_ID`, `KITU_ARENA_CLI_EXECUTABLE` and
`KITU_ARENA_CLI_ARGUMENTS` as in the existing
[Unity replay instructions](../../../apps/demo-game/README.md#tsq1-recordings).
Set `KITU_ARENA_INSPECTION_EVIDENCE_DIR` to an absolute directory for the Inspector
checkpoint captures. Run all `UnityOnlyArena.Tests` PlayMode cases with
`KITU_ARENA_ENCODING=msgpack`; repeat category `ArenaNetwork` with `json`.
Require zero skipped cases rather than treating missing native/host prerequisites
as successful validation.

Open **Project → Arena Inspector** using the
[Admin connection instructions](../../../tools/kitu-web-admin/README.md#inspect-endless-arena).
Match the visible endpoint/session/run/tick to the selected host. Browser polling
only reads; explicit playback controls use the existing owner queue. This evidence
does not claim Windows support, remote operations, CDN delivery, production
deployment or pure gameplay CPU/FPS measurements.
