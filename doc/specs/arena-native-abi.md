# Full application native ABI

Stage 10 of the Endless Arena migration exposes the complete application through
an application-owned `cdylib`/`staticlib`. `apps/demo-game/native` selects the same
`build_arena_runtime` factory used by the server; `kitu-unity-ffi::application`
owns handles, diagnostics, buffering and a replaceable `ApplicationDriver`.
`kitu-transport::wire` owns the typed OSC representation. The common crates do not
depend on the Arena application.

## Version and lifetime

The C declarations and numeric results are authoritative in
`crates/kitu-unity-ffi/include/kitu_application.h`. Ask
`kitu_application_abi_version()` before creating an instance. ABI 1 creation
requires ABI 1 and returns no handle on failure. Configuration is UTF-8 JSON:
empty means embedded TMD defaults; `contractVersion` defaults to 1; optional
`content` contains a detached, validated `ContentVersion`, including its exact
hash, source identity and evaluated values. Unknown fields and invalid content
are refused. Creation diagnostics use a caller-owned buffer and required length.

One live opaque handle owns one application and belongs to the creating thread.
Calls must be serialized on that thread; destruction cannot race another call.
Call destroy once, then never use the pointer again. NULL is detected where
specified, but arbitrary dangling or fabricated pointers cannot be validated by
a C ABI. All non-null memory ranges must be valid, nonoverlapping and remain alive
for the call. Rust never frees caller buffers and C never frees the handle itself.
A caught panic permanently fails the handle; diagnostics and destruction remain
available. The historical movement-only `kitu_*` ABI remains a separate API.

## Input and tick ownership

`submit_json` accepts one JSON request:

```json
{"metadata":{"source":"unity-native","messageId":1,"schemaVersion":1},"bundle":{"messages":[{"address":"/input/arena/start","args":[]}]}}
```

Arguments have explicit tags `int` (i32), `int64` (i64), `float` (finite f32),
`str` (UTF-8 without NUL) and `bool`. Bundles and arguments retain order. Empty
bundles are supported; nested or scheduled bundles are unsupported by the core
OSC-IR model and are rejected, not flattened. Integer consumers must preserve
64-bit values without conversion through a JavaScript floating-point number.
An OSC address begins with `/` and contains no NUL. Metadata may be omitted for
ordinary legacy input; Arena enforces its existing versioned input contract.

Admission returns the Runtime queue sequence and does not advance time. Success
means queued; application acceptance/refusal arrives in the ordinary output
receipts. Invalid structure cannot consume a sequence or modify queued inputs.
Arena retains operation identity and duplicate-consumption protection.

The owner calls `tick` once per 1/60 second of logical time, independently of input
frequency. No sleep or wall clock is hidden in the ABI. Pausing stops game time;
management ticks still advance. Inspection before any tick reports Arena tick
`-1`; the Runtime's internal next-tick counter is not the last applied tick.

## Complete outputs and bounded memory

`read_output` returns UTF-8 JSON `WireBundle[]` for the complete latest tick,
including every state, receipt and game event. It never filters to transforms.
`inspect_json` returns detached application projection bundles without consuming
events or ticking. `last_error` returns UTF-8 diagnostics. Buffers do not include a
NUL terminator; `out_required` is the exact byte count.

Query with NULL and capacity zero, allocate the reported size, then read. A short
buffer writes no partial payload and does not consume output. Successful output
read consumes the whole batch; another read returns EMPTY. Even an empty batch
`[]` must be read before ticking again. A pending batch returns PENDING_OUTPUT
before any clock advance. Tick/serialization failure poisons the handle because
an already-started tick cannot safely be retried as if it had never run.

Limits are 1 MiB per configuration/input request, 4096 queued requests and 16 MiB
of queued serialized input per tick, and 64 MiB per output batch. The application
keeps its own lower structural bounds. Required output pointers and capacities
are checked before mutating operations. Capacity exhaustion returns a diagnostic.

## Verification and native builds

Normal Rust checks run in the repository Dev Container. The native integration
test feeds the frozen preparation and stock scenarios through exported C
functions and compares every tick's complete state/output with the ordinary
server Runtime. The shared oracle also compares C# checkpoints/outcomes using
the established 1e-4 numeric tolerance and exact discrete/event ordering rules.
The stock case reaches 11F, natural death at tick 5526 and retry at 5527.

On macOS with Rust 1.96.0 and Apple command-line SDK tools:

```sh
export CARGO_TARGET_DIR="$PWD/.tmp/native-target"
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export KITU_NATIVE_EVIDENCE_DIR="$PWD/.tmp/native-evidence"
cargo test --locked -p kitu-demo-game-native
cargo build --locked -p kitu-demo-game-native
clang -std=c11 -Wall -Wextra -Werror \
  -I crates/kitu-unity-ffi/include \
  apps/demo-game/native/tests/c_abi.c \
  -L "$CARGO_TARGET_DIR/debug" -lkitu_demo_game_native \
  -Wl,-rpath,"$CARGO_TARGET_DIR/debug" \
  -o "$KITU_NATIVE_EVIDENCE_DIR/c-abi"
"$KITU_NATIVE_EVIDENCE_DIR/c-abi" \
  "$KITU_NATIVE_EVIDENCE_DIR/stock-eleven-death-retry.trace" \
  "$KITU_NATIVE_EVIDENCE_DIR/stock-eleven-death-retry.actual.ndjson"
python3 tools/verify-arena-native.py \
  "$KITU_NATIVE_EVIDENCE_DIR/stock-eleven-death-retry.expected.ndjson" \
  "$KITU_NATIVE_EVIDENCE_DIR/stock-eleven-death-retry.actual.ndjson"
```

The actual C program invokes the built library and checks version/configuration,
input sequence, short-buffer retries, pending-output backpressure, inspection and
creation/destruction. Compare its NDJSON against the ordinary Runtime trace with
`tools/verify-arena-native.py`; generation and comparison must use the same build
and target. Cross-target checks retain the frozen C# tolerance rather than
pretending different execution identities are replay-compatible.

Stage 11 adds the Unity native adapter, standalone packaging and the development
CLI/Admin bridge. That bridge must share the same application host state,
recording/playback and input queue. Server and embedded schedulers must never run
simultaneously against one instance. No structural impossibility is known.
