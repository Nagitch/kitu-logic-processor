# Arena application wire v1

Stage 15 / Issue #158. `/ws/arena` connects Unity to the same Arena application
queue and 60 Hz owner clock used by the C ABI. It has an explicit handshake,
typed inputs, and complete output batches. `/ws/runtime`, its KEP protocol, and
the legacy `/input/move` delta semantics remain available to existing clients.

## Connect and select an encoding

The server runs on port 8787 by default. Start Unity with either:

```sh
KITU_ARENA_WS_URL=ws://127.0.0.1:8787/ws/arena KITU_ARENA_ENCODING=msgpack <Unity-or-Player>
KITU_ARENA_WS_URL=ws://127.0.0.1:8787/ws/arena KITU_ARENA_ENCODING=json <Unity-or-Player>
```

Player arguments `--arena-server ws://127.0.0.1:8787/ws/arena --arena-encoding
msgpack` select the same route. The server backend defaults to MessagePack;
macOS Automatic mode continues to use its embedded native application unless a
server override is supplied. Encoding selection never changes the game clock.

Offer exactly one WebSocket subprotocol:

| Encoding | Subprotocol | Application frame |
| --- | --- | --- |
| JSON | `kitu-arena-json-v1` | UTF-8 text |
| MessagePack | `kitu-arena-msgpack-v1` | Binary named maps |

The first application frame must be Hello within five seconds. There is no
implicit legacy fallback or frame-type based encoding switch. The host checks
compatibility, expected session, producer identity and controller availability
before admitting input or granting ownership.

```json
{"type":"hello","payload":{"compatibility":{"appId":"endless-arena","wireVersion":1,"schemaVersion":1,"presentationVersion":1,"tickRate":60,"features":["output-batches","presentation","replay","typed-osc"]},"clientId":"unity-example","role":"controller","expectedSessionId":null}}
```

`appId`, all numeric versions and tick rate must match. Required features must be
present; additional sorted unique ASCII alphanumeric, `-` or `_` identifiers do
not authorize additional behavior. This dedicated application identity does not
rename the enclosing `demo-game` Runtime. Server Hello includes the session,
granted role, limits, replay/read-only status, and execution package/source hash/
target. Execution identity remains subject to the existing detached replay
checks; a presentation client need not contain the server executable itself.

The native `/host/arena/status` inspection supplies the same compatibility and
execution objects. Unity validates them before submitting native input or
rendering its initial inspection. The C ABI remains version 1 with its existing
ownership, main-thread constraint, request shape, nine exports and byte limits.

## Inputs and complete outputs

Public Rust definitions and bounded codecs live in
[`kitu_transport::application`](../../crates/kitu-transport/src/application/mod.rs).
`kitu_runtime::InputMetadata` and the existing FFI `InputRequest` path reexport
the shared types. Unity's `ArenaWireCodec` implements the same strict named-map
profile; the cross-language corpus is in
[`application-wire`](../../crates/kitu-transport/tests/fixtures/application-wire/manifest.json).

```json
{"type":"input","payload":{"metadata":{"source":"unity-example","messageId":42,"schemaVersion":1},"bundle":{"messages":[{"address":"/input/arena/start","args":[]}]}}}
```

The shared input payload permits omitted/null metadata for generic/native
compatibility. Arena socket admission requires all metadata fields, positive
message ID, the producer bound by Hello, and exactly one valid Arena command.
The `host:` namespace and disconnect management command are reserved. Observer
connections cannot enqueue input or acquire a controller. Native embedded hosts
retain native control; a socket cannot replace it. Replay is read-only.

The Runtime owns queue sequence, applied tick, same-tick order, deduplication,
operation acceptance and rejection. An accepted socket enqueue is not a success
receipt for a consume. The same producer and increasing u64 IDs survive a
reconnect to the same session; inputs are never automatically resent. A new
session requires an explicit new attachment. Unity offers **Connect to new
Runtime** separately from reconnecting the known session.

Every server frame has `{deliverySequence,frame:{type,payload}}`. Sequence starts
at zero for Hello (or fatal handshake Error), increases for every complete
frame, and never wraps. Game tick may move backward during a seek. Transport
sequence is not recorded as game state or used as a simulation clock.

After Hello, an Initial Snapshot contains `{reason,batch,status}`. An Output
contains `{batch,status}`. The batch is `{tick,bundles:[...]}`; every bundle has
its original ordered `messages`. Empty output, an empty bundle, and an empty
middle bundle are distinct. The batch tick equals its paired playback tick.
Unity applies all messages in one received batch on the main thread before
publishing the game and presentation projection, retaining bundle boundaries.

Subscription and initial inspection use one owner-lock boundary and an internal
publication watermark. Outputs and playback status are captured under that same
lock. A committed load, seek (including same/forward tick) or return to live emits
a Seek Snapshot. Snapshot inspection replaces state; it does not replay earlier
sound/notification events. The legacy projection buffer also guards matching
game and presentation simulation steps. No second presentation timer exists.

Standalone Replay frames describe mode changes. Errors contain a stable code,
bounded diagnostic, fatal flag and nullable input ID. Validly decoded input
refusals are nonfatal and do not advance gameplay. Incompatible/malformed
protocols are fatal. Server/client queues are bounded; a lagging connection is
closed with a reconnect/resynchronization diagnostic, releasing only its own
controller. Reconnect supplies a coherent snapshot and live gameplay remains
paused until explicit resume. An observer disconnect does not pause another
controller. There is no in-place Resync command in wire v1.

## Exact scalar types and bounds

OSC arguments retain explicit `int`, `int64`, `float`, `str`, `bool` tags. Int is
signed i32, Int64 signed i64, metadata/delivery IDs and total ticks are u64; none
pass through a floating-point intermediate. JSON uses exact decimal integer
tokens. C# uses long/ulong with checked mathematical range conversion. Browser
`JSON.parse` is not promised to preserve IDs above 2^53; existing Admin uses its
own host APIs rather than consuming this wire format.

MessagePack Float uses the native `0xCA` f32 marker, preserving negative zero,
subnormals and finite extrema. Float64 and integer markers for a typed Float are
rejected. Signed or unsigned compact integer markers are accepted only within
the scalar tag's mathematical range. JSON Float uses finite f32 conversion and
preserves negative zero. Opaque game/presentation JSON remains an OSC string.

| Profile | Bytes, inclusive | Depth | Nodes | Collection entries |
| --- | ---: | ---: | ---: | ---: |
| Network input | 128 KiB | 32 | 16,384 | 8,192 |
| Network output | 8 MiB | 32 | 262,144 | 65,536 |
| Native input | 1 MiB | 32 | 1,048,576 | 1,048,576 |

Native output retains its 64 MiB ABI limit; network parity is promised within
the common valid domain. Root, map keys, values and array elements each consume
one node. Declared lengths, structural budgets and bytes are checked before
allocating attacker-declared collections. Encoders stop at the actual byte
limit and publish only complete frames.

Both formats reject duplicate/unknown fields, positional struct arrays, wrong
scalar types, invalid UTF-8, nonfinite values, unsupported variants and extra
root data. JSON additionally rejects comments, nonstandard quotes and trailing
commas; trailing JSON whitespace is allowed. MessagePack rejects binary,
extension, reserved and Float64 markers. Strings/OSC addresses retain their
existing NUL validation. Fragmented frames count toward one message limit before
append. Oversized traffic terminates the connection; close 1009 is best effort
when unread oversized transport data permits its close frame to flush.

## Verification

The stage verifies Rust-produced JSON/MessagePack fixtures in the actual Unity
Editor, C# re-encoding in Rust, and real JSON/MessagePack sockets against exported
C ABI calls. Comparison covers full typed batches, inspections, receipts,
recorded input IDs/sequences/ticks/order, and TSQ1 bytes. Scenarios include
preparation/real disconnect/reconnect, stock 11F/natural death/retry, edited Rhai
and TSQ1 cues with pause, and integer endpoint/dedup cases. The existing Unity
EditMode/PlayMode and standalone native scenarios remain required.
