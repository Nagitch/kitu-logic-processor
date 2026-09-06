# Kitu Demo Game

`apps/demo-game` is a small application built on top of the Kitu framework crates.
It exists for vertical-slice development, Web Admin hosting, and CI scenario tests.

## Layout

- `src/lib.rs`: app-level runtime construction and project action loading.
- `src/bin/admin_host.rs`: local HTTP/WebSocket host used by Web Admin.
- `kitu-app-actions.toml`: project-owned app action manifest.
- `scenarios/`: checked-in scenario fixtures for CI.
- `tests/`: scenario test harnesses that execute fixtures against the Kitu runtime.
- `docker-compose.yml`: standalone app host plus Web Admin frontend.

## Run

From the repository root:

```sh
cargo run -p kitu-demo-game --bin kitu-demo-game-admin-host
```

Then open the Web Admin frontend separately, or use the app compose file:

```sh
docker compose -f apps/demo-game/docker-compose.yml up --build
```

Endpoints:

- Web Admin: http://localhost:5173
- Demo game admin host: http://localhost:8787
- Health: http://localhost:8787/health
- Web Admin WebSocket: ws://localhost:8787/ws
- Arena client WebSocket: ws://localhost:8787/ws/arena
- Generic Unity/runtime WebSocket: ws://localhost:8787/ws/runtime
- Experimental WebTransport gateway: https://localhost:9443 over UDP

The `/ws/runtime` endpoint is the development-time Unity vertical slice. It
accepts OSC-IR JSON messages directly, including `/input/move`, advances the
same runtime used by the embedded path, and broadcasts `/render/player/transform`
responses for presentation clients. It also forwards world `state` snapshots so
Unity can mirror Web Admin object spawn/move/reset actions.

The WebTransport gateway is a separate local-development container. It receives
KEP MessagePack envelopes, decodes OSC packet payloads, and relays them to the
existing Web Admin WebSocket endpoint over the Docker internal network. The
existing WebSocket endpoints remain the fallback and comparison path.

## Scenario tests

The app scenarios are ordinary Rust tests and run in the workspace CI:

```sh
cargo test -p kitu-demo-game
```

## Boss scripts

`content/boss.rhai` supplies the default boss decisions through the real bounded
Rhai host. Set `KITU_ARENA_SCRIPT` to an editable absolute source path, then use
Admin **Game Scripts** or `kitu-cli script validate` / `script stage <hash>`.
The active run keeps its current source; start/retry adopts the staged version.
Recordings preserve source and policy independently of authoring files. See the
[boss contract, diagnostics and native options](../../doc/specs/arena-boss-scripts.md).

## Endless Arena migration

The same host also runs the Arena application at 60 Hz independently of messages.
Open `Assets/KituDemoApp/EndlessArena/KituEndlessArena.unity` in the Unity demo
project and select its Server backend at `ws://localhost:8787/ws/arena`.
The macOS Automatic backend embeds Kitu unless an external server is selected.
The default scene
plays the complete endless game, including boss rewards, results and retry. The
original `EndlessArena.unity` and frozen C# fixtures remain the comparison oracle.

WASD moves, mouse buttons fire the two weapons, Z/X use consumables, E opens a
nearby chest, I/Tab opens inventory, and Escape closes an overlay or pauses.
The client only submits input and renders server state. See the
[Arena contract](../../doc/specs/arena-runtime-contract.md) and
[full-game evidence](../../doc/verification/arena-progression/results.json).

To include the live migration scene in PlayMode validation, run the host and set
`KITU_ARENA_WS_URL=ws://localhost:8787/ws/arena` when launching Unity tests.
Without that variable, the external-host connection test is explicitly skipped.

## Arena inspection

Open Admin **Project → Arena Inspector** to observe the selected host's game
state, entities, minimap, events, presentation cues and host owner-update timing.
Every panel uses one verified snapshot from `GET /arena/inspection`; its session,
run and tick distinguish live play, replay and a completed seek. Select entities
from the map or list, and use the existing playback controls to inspect exact
ticks after loading a recording through **Arena Replay**. Failed refreshes retain
the last snapshot with a stale status.

The embedded macOS Player exposes the same endpoint through its optional bridge
at `http://127.0.0.1:8789`. Inspection reads neither advance the game nor claim a
controller. The generic World view remains separate. See the
[inspection contract](../../doc/specs/arena-inspection.md) for exact integer
representation, event retention, timing scope and replay replacement semantics.

## Tanu parameters

`content/arena.tmd` is a real Tanu container with four managed Formula tables:
`items`, `enemies`, `difficulty`, and `chests`. It is read and evaluated through
Tanu's public `tmd-core` API, pinned at
`194358e8791f1391492abcb60d8cfcc37bbb383a` (the parent workspace's Tanu revision).
The default values match the preserved Unity-only reference. For example,
Quick Blade's boss damage is the Formula `D2 * 1.2`, evaluated as 24.

1. Open `content/arena.tmd` with **Tanu Markdown Editor** in VS Code. If VS Code
   initially chooses its binary/text viewer, use **View: Reopen Editor With...**
   and select Tanu Markdown Editor. The parent workspace's Dev Container includes
   the CLI/editor preparation tasks. This workflow needs that same compatible Tanu version.
2. In **Table**, select a source, select a cell, edit its value or Formula, then
   save the document. Numeric Formula input in the editor begins with `=`.
3. Open Admin's **Game Parameters** page and choose **Reload and validate**. Inspect the
   evaluated tables and candidate hash. Formula errors, wrong types, duplicate
   identifiers, out-of-range values, or dangling chest item references reject
   the whole candidate. Tanu itself also refuses to publish invalid Formula documents.
4. Choose **Apply to next run**. The host queues the exact reviewed values through
   the Runtime's reserved management input. The current run keeps its values,
   including while paused. Return to the opening screen and start, or retry after
   death, to activate the pending version without restarting Unity.

The host reads `apps/demo-game/content/arena.tmd` relative to its working directory;
set `KITU_ARENA_CONTENT` to another `.tmd`, `.sqlite` or `.arena.json` file if needed.
The legacy `KITU_ARENA_TMD` alias remains a fallback when `KITU_ARENA_CONTENT` is unset.
Validation does not automatically
apply a file. The initial pending version is bundled into the application; after
a host restart, validate and apply any local edits again.

Each start emits `/game/arena/run` with the complete evaluated configuration and
hash. The host writes an atomic JSON manifest under
`apps/demo-game/.arena/runs/<runtime-session>/<run>.json`; use
`KITU_ARENA_RUN_DIRECTORY` to choose the directory. Admin reports save failures.
TSQ1 recordings also retain these full evaluated run configurations; see the recording workflow below.

HTTP interfaces use the same host as Unity:

| Endpoint | Behavior |
|---|---|
| `GET /arena/content` | Current and next-run values, last candidate, source snapshots, field origins, candidate differences, diagnostics, saved-run status |
| `POST /arena/content/validate` | Read the configured file and evaluate outside the simulation lock |
| `POST /arena/content/stage` | Queue a reviewed candidate; JSON body contains `hash` and `sourceSha256` |

An outdated candidate identity returns HTTP 409. An invalid document returns
diagnostics with no applicable candidate; active and pending versions are retained.
The legacy key/value parser in `kitu-data-tmd` remains available for old tests but
is not used by Arena.

Run validation in the Dev Container:

```sh
cargo run -p kitu-demo-game --bin arena-content -- validate apps/demo-game/content/arena.tmd
cargo test -p kitu-demo-game -p kitu-data-tmd -p kitu-runtime
cd tools/kitu-web-admin/frontend
pnpm check && pnpm lint && pnpm build
```

`arena-content create-reference <path>` generates a new reference-valued TMD for
development. The separate [live CLI](../../doc/specs/live-shell.md) operates the running host.
The macOS PlayMode
test `LiveTanuConfigurationIsProjectedInNewRuns` accepts
`KITU_ARENA_EXPECTED_STARTER_DAMAGE` (default 20) to verify a value applied through
Admin. See [stage 6 evidence](../../doc/verification/arena-tanu/results.json).

## SQLite and layered parameters

The same four typed tables can come from real SQLite. A source plan combines a
complete base with sparse difficulty, event and debug overrides. TMD layers use
the public Tanu Formula evaluator; SQLite layers retain their native scalar types.
The fixed order is `base → difficulty → event → debug`, independent of JSON key order.

Generate editable samples in the Dev Container from the Kitu repository root:

```sh
mkdir -p .tmp/content
cargo run -p kitu-demo-game --bin arena-content -- create-sqlite .tmp/content/default.sqlite
cargo run -p kitu-demo-game --bin arena-content -- create-plan .tmp/content/sample.arena.json
cargo run -p kitu-demo-game --bin arena-content -- validate .tmp/content/sample.arena.json
KITU_ARENA_CONTENT="$PWD/.tmp/content/sample.arena.json" cargo run -p kitu-demo-game --bin kitu-demo-game-admin-host
```

The SQLite-only sample exactly preserves reference values. The four-layer sample
deliberately changes starter damage from base 20 through 24 and 28 to debug 32.
Edit its TMD sources in VS Code or its SQLite scalar cells with a SQLite editor.
Reload in **Game Parameters**, inspect the sources, hashes, field origins and
differences against active/pending values, then apply the candidate to the next run.
An invalid edit clears the candidate and reports diagnostics while retaining the
last valid active and pending versions. Stale source hashes are rejected even when
the evaluated values have not changed.

For an embedded macOS player, pass `--arena-content /absolute/path/sample.arena.json`
with its development bridge enabled. The same host APIs operate that native Runtime;
no external server is required. Source files are read only on explicit validation.
Saved run and TSQ1 data contain evaluated values and detached provenance, so replay
does not reopen authoring files. Different execution builds still require the
matching archived runtime. See the [source contract](../../doc/specs/arena-content-sources.md)
for SQL schema, patch identities, limits and compatibility.


## TSQ1 recordings

The host records from its first Runtime tick, including pauses and retries. Save
and verify a live session through the actual binary TSQ1 APIs:

```sh
curl -X POST http://127.0.0.1:8787/arena/recording/save
# Use the returned SHA-256 id:
curl -X POST http://127.0.0.1:8787/arena/recordings/ID/verify
curl http://127.0.0.1:8787/arena/recording/export -o arena.tsq
curl -X POST --data-binary @arena.tsq http://127.0.0.1:8787/arena/recordings/import
```

Verification runs a separate instance of the same Runtime, restoring saved
initial values and replaying normal input admission/ticks. It compares every
state and ordered output, then returns the final state; the live game continues.
Changing the current Tanu document does not change the saved configuration.
`GET /arena/recording` exposes limits and recording failures. Initial limits are
one hour of management ticks and 64 MiB per encoded file; start a new host session
for a fresh recording. Invalid/incompatible files return a diagnostic.

See the [full file/version/endpoint contract](../../doc/specs/arena-replay.md).
Open **Project → Arena Replay** in Admin to save the live session or import a
`.tsq`. Choose **Verify and load**, then use Play, Pause, Step one tick, Stop or
Seek. The loaded recording starts at tick -1; Stop returns there. The connected
Unity scene becomes a read-only replay view. Return to live restores the parked
paused run; resume explicitly in Unity. Game Parameters cannot apply edits while
replaying. The full status JSON exposes the recording's state and content hash.

To repeat the real-scene replay test, regenerate/import the stock recording for
the current build (see the contract), then add its ID to the PlayMode environment:

```sh
KITU_ARENA_WS_URL=ws://127.0.0.1:8787/ws/arena \
KITU_ARENA_REPLAY_ID=<imported-stock-content-id> \
  <Unity-editor> -batchmode -projectPath <isolated-unity-project> \
  -runTests -testPlatform PlayMode -assemblyNames UnityOnlyArena.PlayModeTests \
  -testResults <results.xml> -logFile <editor.log>
```

The test checks step/play/pause, 11F death at tick 5526, retry at 5527, complete
Admin/Unity state equality, stop/reconnect and paused live restoration. Stage 9 adds the [live CLI and browser Shell](../../doc/specs/live-shell.md).
Run `kitu-cli help`, `app action run arena.start`, `inspect application` or
`scenario run preparation-smoke` against this host; the same commands are
available in **Kitu general → Shell**. Their JSON results include actual applied
ticks or refusal reasons. `replay load <id>`, `replay seek 5526` and `replay step`
control the connected Unity replay.

## Native application library

The complete Arena application can also run through the C ABI in
[`native`](native/README.md). The application factory uses the same Runtime and
embedded TMD as the server; typed input, tick, complete output and state inspection
are verified against all stock scenario states/events and the frozen C# oracle.
The [native ABI contract](../../doc/specs/arena-native-abi.md) includes macOS build
and actual C-caller commands. The macOS standalone embeds this library and exposes
an optional local Admin/CLI bridge driven by Unity's single clock owner.

## TSQ1 presentation authoring

Boss warnings and floor transitions use the real files in `content/timelines/`.
Generate editable copies with `cargo run --locked -p kitu-demo-game --bin
arena-timelines -- /absolute/clip-directory`, then set
`KITU_ARENA_TIMELINE_DIRECTORY` on the server or pass `--arena-timeline` to Unity.
Use **Story Sequencing** or `timeline validate` / `timeline stage <hash>` to adopt
reviewed edits on the next run. `inspect timeline` shows the observed run's clip
versions and exact cue positions. The [presentation contract](../../doc/specs/arena-presentation-timelines.md)
documents binary authoring options, bounds, trigger rules and detached replay.

The scene replay test also seeks the first floor cue at tick197 and a boss warning
at1649, compares Admin/Unity presentation, verifies pause and reattachment, then
steps the same warning and continues to the existing death/retry checks.
