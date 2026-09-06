# Stage 15: JSON, MessagePack and C ABI parity

[Issue 158](https://github.com/Nagitch/kitu-logic-processor/issues/158) /
[PR 159](https://github.com/Nagitch/kitu-logic-processor/pull/159).
The [wire contract](../../specs/arena-application-wire.md) documents the public
API and compatibility/bounds policy. [results.json](results.json) records the
implementation revision, tool evidence, artifact identities and remaining merge
work without treating local tests as a merged stage.

| Verification | Result |
| --- | --- |
| Implementation commit CI | All five jobs passed; 330 workspace tests/doctests plus one separately repeated scenario test |
| Dev Container workspace | 329 tests/doctests in 56 suites; zero failed/ignored |
| Final controller-release change | 30 host regressions and 9 real-socket tests; all passed |
| Rust tooling | format, all-target/all-feature Clippy and rustdoc with warnings denied passed |
| Actual Unity 6000.6 | 119 EditMode, 22 complete MessagePack PlayMode and 6 JSON network PlayMode passed |
| Final server / immediate reconnect | Both encodings passed four immediate reconnects with queued input and explicit resume |
| Rust/C# codec | 40 shared cases; all 11 valid C# JSON/MessagePack pairs read back identically in Rust |
| Actual JSON socket / MessagePack socket / exported C ABI | 7,390 ticks, 7,445 inputs, 88 receipts; exact complete batches, inspection and recorded input/order |
| macOS native tests | 23 tests/doctests; zero failed/ignored |
| Actual C caller and graphical native Player | Each matches all saved state/output for 28 preparation, 5,528 stock death/retry, 1,800 edited Rhai, 1,830 edited timeline/pause ticks |
| Local Player images | All 15 byte-identical to the previously inspected Stage 14 captures |

The full workspace run began before the final close-acknowledgment ordering
regression was included. The final host and real-socket runs, fresh server,
Unity reconnect cases and newly built Player cover that final change. Clippy
and rustdoc compiled the final source. [Implementation CI](ci-implementation.json) ran all five jobs successfully on the exact implementation commit, including the final close-order regression. The evidence-only follow-up also receives the required PR checks.

## What was exercised

The real-socket tests negotiate both fixed subprotocols and reject wrong app,
wire/schema/presentation versions, rate and missing capabilities before taking
control. They cover observers, embedded ownership, producer spoofing, bounded
input pressure, lag closure, real fragmented frames, oversized input,
subscription/snapshot races, backward seek and reconnect deduplication.

The parity harness uses two manually ticked real TCP/WebSocket hosts and the
exported C ABI on its creating thread. It compares every full output bundle,
inspection and receipt, then inspects the actual saved TSQ1 input metadata,
scalar bits, queue sequence, applied tick and within-tick order. The socket
recordings also have identical bytes. A single bound producer preserves duplicate
identity while frozen C# game expectations are checked separately. Preparation
disconnect uses real socket closure and the corresponding observed native
management receipt; it does not impersonate a reserved producer on the socket.

Codec fixtures cover exact signed/unsigned endpoints, positive compact Int64,
negative-zero f32, subnormal/extreme f32, Unicode, empty/multiple bundles and
arbitrary map order. Invalid fixtures cover unknown/duplicate/missing fields,
wrong variants, overflow, integer negative-zero, invalid UTF-8, unsupported
MessagePack markers, array-as-struct, trailing roots and truncated/huge lengths.
Unity additionally checks inclusive byte bounds, structural budgets and each
compatibility dimension. The C# exports are local, hashed in [codec.json](codec.json),
and were explicitly consumed by Rust; CI without that optional environment
variable does not claim to run the Unity Editor.

Unity's normal scene checks live CLI/shared Shell, Tanu defaults, movement,
settings/pause, inventory/consumables/combat, 11F/natural death/retry and exact
replay/step/backward-seek projections. Native inspection validates the same
compatibility object before use. The final reconnect regression queues 16 frames,
immediately disconnects/reconnects four times per encoding, retains the session,
and observes a stopped game clock until explicit resume.

## Findings fixed during verification

- The shared nullable Hello field needed an explicit JSON null in C#; a nullable
  string creates a different `JValue` kind. Golden coverage now verifies both.
- Encoding now follows declared map-field order while decoding accepts arbitrary
  order, so C# emits the same canonical MessagePack as Rust.
- Unity closes through its single writer and waits for completion before opening
  the replacement connection. The server releases that controller before its
  Close reply can be polled; an old lease cannot release the replacement. A
  deterministic test verifies this boundary in addition to actual sockets and
  both Unity encodings. Abrupt/oversized traffic still terminates within bounds.

## Reproduce

Run ordinary Rust checks in the repository's Dev Container:

```sh
cargo fmt --all -- --check
cargo test --locked --workspace --all-features
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --all-features --no-deps
KITU_WIRE_EVIDENCE_DIR="$PWD/.tmp/wire-proof" cargo test --locked -p kitu-demo-game-native --test wire_parity -- --nocapture
```

Run Unity EditMode with `KITU_ARENA_WIRE_EVIDENCE_DIR` set to an absolute directory
outside Assets. Then, from the container (using its corresponding mounted path):

```sh
KITU_APPLICATION_WIRE_CSHARP_FIXTURES="$PWD/.tmp/unity-wire" cargo test --locked -p kitu-transport csharp_reencoded_frames_decode_to_identical_typed_values_and_float_bits -- --nocapture
```

For real scenes, start the Arena host, set `KITU_ARENA_WS_URL` to its `/ws/arena`
endpoint and run `UnityOnlyArena.Tests` PlayMode with `KITU_ARENA_ENCODING=msgpack`
and `json`. CLI/replay cases additionally use the isolated CLI executable/
arguments and a freshly verified stock recording as documented by those test
fixtures. Use the [macOS build steps](../../../kitu-integration-runner/unity-demo-game/README.md#reproduce-the-embedded-macos-build)
for native and Player validation. Apple SDK work remains on macOS.

[Native packaging](native-package.json), [Player packaging](player-build.json),
[C caller](c-caller.json), [Unity cases](unity-tests.json),
[socket/FFI parity](parity-results.json), and [image hashes](visual-equivalence.json)
retain the concrete checks. The frozen Unity-only revision is unchanged. Exact
Linux/macOS binaries and the signed Player were retained locally for older
recordings; [runtime-archive.json](runtime-archive.json) identifies every file.
No additional PNGs were uploaded in this stage.
