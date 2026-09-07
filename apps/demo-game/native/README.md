# Endless Arena native library

This application-owned crate builds `kitu_demo_game_native` as a dynamic library,
static library and Rust library. It creates the same complete Arena Runtime as
the server. The generic ABI and C header live in `kitu-unity-ffi`; no Arena rules
are duplicated in the native crate.

Create with ABI 1 and empty configuration to use the embedded, real Tanu TMD.
Alternatively pass `{"contractVersion":1,"content":<ContentVersion>}` with a
previously evaluated, hashed configuration. Invalid values, hashes, unknown
fields and incompatible contracts are rejected before returning a handle.

Unity's packaged Player instead supplies an absolute `bundledContentDirectory`.
Its five-file source package initializes evaluated Tanu configuration, Rhai
boss rules and TSQ1 clips before tick 0. It cannot accompany detached `content`,
`script` or `timeline` initializers, and an explicit invalid package never falls
back to compiled defaults. Optional `expectedBundledContentHash` binds creation
to a previously reviewed manifest: Unity sends it automatically to reject
replacement during asynchronous asset loading, before Runtime or storage
creation. Native host inspection reports the captured `package` identity
separately from game state. See the
[packaged content contract](../../../doc/specs/arena-packaged-content.md) for
schema, byte limits, hashing and reproducible Addressables/Player builds.

The caller owns the one 60 Hz scheduler, submits typed input without advancing
time, ticks once and retrieves the complete output batch. Inspect is read-only.
A short read does not consume output; the next tick is refused until the previous
batch is retrieved. Destroy on the owning thread exactly once.

An optional development bridge exposes the same host, content catalog, recorder,
playback and Shell command history to HTTP/WebSocket clients:

```json
{
  "contractVersion": 1,
  "bridge": { "enabled": true, "address": "127.0.0.1:8789" },
  "storageDirectory": "/absolute/path/to/arena"
}
```

Only literal loopback addresses are accepted. Port `0` allocates a free port;
`kitu_application_inspect_host_json` reports the actual `bridgeEndpoint` and the
same `sessionId` used by the CLI catalog. Binding failure rejects creation with
a diagnostic. Empty/default configuration starts no listener and writes no files.
An optional absolute `contentPath` chooses the `.tmd`, `.sqlite` or `.arena.json`
source plan evaluated by Admin through the [shared typed loader](../../../doc/specs/arena-content-sources.md).
The same configuration accepts an absolute `scriptPath` for editable boss Rhai
source. Otherwise the storage directory receives `boss.rhai` only when absent.
CLI/Admin validation and staging apply edits to the next run. Detached `script`
may accompany detached `content` at creation; these saved versions remain
authoritative even when authoring files differ. See the
[boss script contract](../../../doc/specs/arena-boss-scripts.md).

`timeline` optionally supplies a detached validated presentation version;
`timelineDirectory` selects an absolute authoring directory containing fixed
`boss-telegraph.tsq` and `floor-transition.tsq` files. Embedded storage seeds
default editable copies in `timelines/` only if absent. External directories and
detached factory clips are not replaced. CLI/Admin validation and staging adopt
edits only on the next run; saved bytes reproduce presentation after deletion.
See the [TSQ1 presentation contract](../../../doc/specs/arena-presentation-timelines.md).
Otherwise the storage directory receives an editable `arena.tmd` on first use;
existing documents are preserved. Validation and staging keep next-run semantics.

With package initialization, absent default authoring files are seeded from
the package's captured bytes; existing edits are never overwritten. Authoring
files do not replace the package's initial versions until explicitly validated
and staged for a later run. A Unity Player can select a different complete
package with `--arena-package /absolute/package-directory` and separate writable
storage with `--arena-storage /absolute/directory`. Saved replays keep their
detached versions after package or authoring changes.

The bridge never advances the clock. Unity/another native owner continues calling
tick while paused or detached, allowing operator receipts and replay controls to
complete. WebSocket clients observe the native session and cannot become its game
controller. Host metadata is separate from deterministic game state and output.
Destruction stops admission and observers, cancels replay workers between ticks,
and joins I/O work before returning, including when called inside a Tokio context.

See [`arena-native-abi.md`](../../../doc/specs/arena-native-abi.md) for the complete
contract, reproducible builds and full-scenario C verification. The
[`embedded_bridge`](tests/embedded_bridge.rs) integration tests use actual HTTP
and WebSocket clients to verify clock ownership, content/replay sharing and teardown.

The [Stage 11 verification record](../../../doc/verification/arena-embedded/README.md)
includes the actual ARM64 library, C caller and graphical Unity standalone runs.
Both preparation and the complete 11F/death/retry input traces reproduce every
state and output through the bundled library with the external server stopped.
The full Unity suites also passed: 50 EditMode and 21 PlayMode tests, including
native lifecycle, shared bridge/replay and both complete stock-run backends.
