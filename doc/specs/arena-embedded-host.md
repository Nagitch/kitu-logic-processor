# Embedded Arena host

Stage 11 extends the [native ABI](arena-native-abi.md) with a shared development
host. Application rules still live in `apps/demo-game/src/arena`; native and
standalone server hosting use `apps/demo-game/src/host`. The native factory and
optional listener are application-owned. Generic C buffer/lifetime behavior
remains in `kitu-unity-ffi`.

## Ownership

`ArenaHost` exclusively owns the right to tick one Runtime, together with its
recorder, playback state, content catalog and command receipts. Creating a router
starts no scheduler. The server supplies a 60 Hz timer; an embedded caller supplies
the ticks through the existing C ABI. HTTP/WS handlers admit work or inspect
state. Owner ticks commit playback transitions and prepared seek results.

Original OSC bundle boundaries and complete output order pass through the host
unchanged. Recording captures live outputs and inputs before output presentation;
historical playback cannot overwrite a live run manifest. The native adapter
must drain a complete output batch before requesting the next tick.

## Host inspection

`kitu_application_inspect_host_json` is an additive ABI 1 operation using the same
length queries, caller buffers, size limit and thread constraints as game
inspection. Drivers without host metadata return `[]`. Arena returns one bundle
with `/host/arena/status` and a single `str` argument containing this JSON object:

| Field | Meaning |
|---|---|
| `sessionId` | Host lifetime identity, also returned by `/shell/catalog` |
| `schemaVersion` | Arena contract version |
| `playbackMode` | Existing replay mode object: active, playing, seeking, tick, totalTicks, recordingId, error |
| `readOnly` | Gameplay input is currently refused, including pending replay activation |
| `bridgeEndpoint` | Actual loopback HTTP endpoint, or null when disabled |
| `closing` | Shutdown has begun |

This information is inspected under the same host lock. Session identities and
tooling state do not enter deterministic game projections or replay proof hashes.

## Native configuration and storage

Empty factory configuration keeps embedded TMD defaults, no listener and no file
writes. `bridge.enabled` enables a listener; `bridge.address` defaults to
`127.0.0.1:8789` and accepts a literal loopback address, including port zero.
Requested binding failures reject creation rather than attaching clients to a
different process. The bridge is for local development, without production auth.

`storageDirectory` and `contentPath`, when provided, must be absolute paths.
Storage contains the editable `arena.tmd`, immutable `runs/` manifests and
`recordings/` TSQ1 files. Existing authoring documents are never overwritten on
startup. An explicit `contentPath` selects a separate authoring document.
Source documents are evaluated only through the existing validate/stage flow;
the current run retains its pinned content. Saved detached configuration passed
to the factory remains authoritative for that initial Runtime.

## Detach and shutdown

A native gameplay connection can detach without destroying its Runtime: queue
`/input/arena/disconnect`, clear continuous input, keep management ticks running,
and retain the host/session identity. Reattach inspects current state and requires
an explicit resume. Observer WebSocket disconnection cannot pause the native run.
Legacy WebSocket mutations are refused for an embedded host.

Actual destruction closes admission, notifies observers and cancels background
verification/seek work between ticks. Every host-owned task and the listener are
joined before C destruction returns. Unfinished HTTP requests are cancelled after
admitted host work drains, so incomplete headers/bodies cannot block teardown.
Tokio is shut down on a plain scoped thread
so callers inside an existing async runtime do not trigger a nested-runtime panic.
No timeout abandons native code that could execute after library unloading.

## Validation

`cargo test -p kitu-demo-game-native --test embedded_bridge` uses a real ephemeral
loopback listener. It covers shared session identity, no autonomous clock, Shell
deduplication, native detach/resume, Tanu next-run activation and invalid edits,
old-recording verification, TSQ1 playback/step/seek/live, observer disconnect,
listener release/rebind, incomplete HTTP requests, unread observers, invalid binds
and destruction inside an existing Tokio context. The full native stock comparisons continue using the frozen C# oracle
and ordinary Runtime outputs without host metadata.
