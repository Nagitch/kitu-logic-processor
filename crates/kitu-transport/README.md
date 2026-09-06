# kitu-transport

Transport abstraction and adapters for moving OSC/IR messages between the runtime and external systems.

## Responsibilities
- Define the `Transport` trait for sending and receiving OSC/IR envelopes.
- Provide composable adapters (in-memory, network, etc.) without dictating routing policies.
- Keep the runtime deterministic by clearly separating transport concerns from game logic.

## KEP and OSC helpers

`kitu-transport` provides MessagePack KEP helpers:

- `KepEnvelope`
- `encode_kep_envelope`
- `decode_kep_envelope`

It also provides OSC packet helpers for the OSC-IR model:

- `encode_osc_packet`
- `decode_osc_packet`
- `encode_osc_bundle`
- `decode_osc_bundle`

Supported OSC message argument types:

- `i`: int32
- `h`: int64
- `f`: float32
- `s`: string
- `T` / `F`: bool

Supported OSC packet shapes:

- Single OSC messages.
- OSC bundles containing message elements with the immediate timetag.

Nested OSC bundles, blobs, arrays, and additional scalar tags remain unsupported
until a concrete Kitu client or runtime path requires them.

## Typed JSON, MessagePack, and FFI values

`wire::{WireBundle, WireMessage, WireArg}` supplies a shared Serde representation:

```json
{"messages":[{"address":"/example/count","args":[{"type":"int64","value":1}]}]}
```

Tags `int`, `int64`, `float`, `str`, and `bool` preserve OSC scalar types. Message
and argument order, empty bundles, and empty argument lists are retained. Unknown
fields and unsupported argument tags are rejected. The representation has no
nested bundles or timetags; tick scheduling belongs to the runtime envelope.

Use `WireBundle::try_from(&osc_bundle)` for output and
`OscBundle::try_from(wire_bundle)` after deserializing input. These conversions
require finite float32 values, addresses starting with `/`, and no embedded NUL
bytes in addresses or string arguments. Deserialization alone does not validate
all these conditions. Adapters apply their own payload limits and application
admission rules. Existing binary OSC helpers retain their compatibility behavior.

JSON consumers must preserve signed 64-bit integer values; converting the JSON
through a JavaScript `Number` can lose precision. MessagePack and native Rust
decoding retain the integer width without inference.

For output serialization, `WireBundleRef::new(&bundle)` and
`WireBundlesRef::new(&bundles)` borrow the original OSC values. They produce the
same wire representation while validating each visited value without cloning
messages, strings, or the complete batch. Serialize these views directly into
a bounded writer to enforce output limits before allocating another copy of a
large batch. On serialization failure, discard any prefix written so far.

## Bounded application wire 1

`application::Codec` provides complete JSON or MessagePack client/server frames.
Both encodings use named maps and adjacent `{type,payload}` unions. This is a
typed application protocol; its MessagePack payload is not JSON text or a KEP
envelope. Existing KEP and raw `wire` APIs remain compatible.

```rust
use kitu_transport::application::{Codec, Encoding, InputFrame, NETWORK_INPUT_LIMITS};
use kitu_transport::wire::WireBundle;

let codec = Codec::new(Encoding::MessagePack, NETWORK_INPUT_LIMITS).unwrap();
let input = InputFrame { metadata: None, bundle: WireBundle::default() };
let bytes = codec.encode_input(&input).unwrap();
assert_eq!(codec.decode_input(&bytes).unwrap(), input);
```

`InputFrame` shares the existing native `{metadata,bundle}` shape, and
`InputMetadata` is reexported from `kitu_runtime::InputMetadata`. Bare native
inputs may still omit metadata; the shared framed Input payload accepts the same
omission as `null`. Serialization emits an explicit `null`. Application admission
may require metadata and must
validate source identity, permissions, command count and the application schema.
The generic codec validates OSC structure before returning a decoded frame.

The network input profile is 128 KiB, depth 32, 16,384 nodes and 8,192 entries
per collection. The output profile is 8 MiB, depth 32, 262,144 nodes and 65,536
entries per collection. A root, each map key/value and each array element count
as nodes. Limits include the entire encoded frame. A structural scan checks
declared lengths, node/depth budgets, UTF-8, duplicate keys and exact consumption
before DTO allocation. Struct arrays, unknown fields/tags, extensions, blobs,
nonfinite values and trailing roots are rejected. Only trailing JSON whitespace
is accepted. The native request profile remains 1 MiB; its existing bare output
ABI retains the separate 64 MiB limit.

MessagePack encodes typed floats with the float32 marker and rejects float64 or
integer markers in a typed float. JSON typed floats decode to finite f32 values.
Finite JSON number lexemes are bounded by the frame size rather than an additional
token-length limit, including long decimal spellings of the same f32 value.
Both preserve negative zero, subnormals and f32 extremes. Compact positive
MessagePack integers can represent `int64` arguments: the explicit OSC tag,
not the integer marker width, determines the logical type. Signed arguments and
unsigned metadata/delivery identifiers retain their full i64/u64 ranges. JSON
clients must parse these integer tokens exactly without an intermediate double.

`Compatibility` and `ExecutionVersion` are shared inspection/greeting DTOs.
`check_compatibility` requires exact application, wire, schema, presentation and
tick-rate values plus every required feature. Extra advertised features do not
authorize use. Feature names are sorted unique ASCII identifiers. Execution
identity is observable; the application owns replay-version validation.

`ServerFrame.deliverySequence` is socket-local and independent of replay ticks.
`OutputBatch` retains empty and multiple bundles. Output/snapshot status must
refer to the same tick as its batch. `ServerFrameRef` and the borrowed batch/frame
types encode original `OscBundle` slices incrementally through a capped writer.
They produce exactly the owned frame's bytes without cloning the batch or its
strings. Hosts publish only the complete returned byte vector. Handshake,
controller ownership, fixed WebSocket subprotocol selection, lag termination,
reconnection and snapshot publication barriers remain host responsibilities.

The shared [fixture manifest](tests/fixtures/application-wire/manifest.json)
contains valid paired JSON/MessagePack cases and malformed single-format cases
for Rust and C# parity checks. Ordinary tests verify their bytes and classification.
After an intentional contract update, regenerate them explicitly with
`UPDATE_APPLICATION_WIRE_FIXTURES=1 cargo test -p kitu-transport application::tests::shared_application_wire_fixtures`.
The `csharp_reencoded_frames_decode_to_identical_typed_values_and_float_bits`
test reads fresh external Unity exports from `KITU_APPLICATION_WIRE_CSHARP_FIXTURES`.
Set that variable and run it with `-- --nocapture` after the Unity fixture tests; it requires
every valid JSON/MessagePack pair and compares canonical MessagePack after Rust
decoding, including the sign bit of floating-point zero. Exported evidence bytes
remain outside the repository fixtures. Without the variable, this extra external
verification does not run; the ordinary corpus tests always run.

## Publish readiness
- Status: internal-only (`publish = false`) while the MVP takes shape; metadata now aligns with crates.io requirements.
- Before enabling publication, run the workspace gates:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Transport positioning within the runtime: `doc/crates-overview.md`
