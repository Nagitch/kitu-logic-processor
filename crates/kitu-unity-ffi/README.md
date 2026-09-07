# kitu-unity-ffi

C ABI bindings that expose the Kitu runtime to Unity hosts.

## Responsibilities
- Marshal data between Unity buffers and the Rust runtime API.
- Provide a stable ABI surface appropriate for a `cdylib` target.
- Keep Unity-facing details isolated from core runtime logic.

## Full application ABI

The `application` module provides an application-independent C boundary. Each
application owns its native library and exports the thin wrappers declared in
[`include/kitu_application.h`](include/kitu_application.h). Its factory installs
the same rules used by its server, using `RuntimeDriver` for an ordinary
`Runtime<LocalChannel>` or `ApplicationDriver` for an application-owned host.
This crate does not depend on Endless Arena or any other application.

- `kitu_application_abi_version` and `kitu_application_create` negotiate ABI 1 and
  construct a run. Configuration is application-owned; an empty buffer can select
  that application's defaults. Creation failures return diagnostics without a handle.
- `kitu_application_submit_json` validates and admits one ordered OSC bundle with
  optional producer metadata. Queue admission and application acceptance are
  separate: the next authoritative tick emits the application command receipt.
- `kitu_application_tick` advances exactly one management tick. The embedding host
  schedules the application's agreed tick rate independently of input/render frequency.
- `kitu_application_read_output` drains a complete tick's ordered bundle array.
- `kitu_application_inspect_json` copies a detached projection without changing state.
- `kitu_application_last_error` exposes failure diagnostics;
  `kitu_application_destroy` releases the run on its creating thread.

Typed JSON uses the shared `kitu-transport::wire` representation, preserving i32,
i64, finite f32, string and boolean types and bundle/message/argument order:

```json
{
  "metadata": { "source": "unity", "messageId": 1, "schemaVersion": 1 },
  "bundle": {
    "messages": [{ "address": "/input/arena/start", "args": [] }]
  }
}
```

Unknown fields are rejected. Metadata, when present, requires a producer of
1–128 UTF-8 bytes without NUL, a positive message ID, and a positive application
schema version. The application enforces its actual contract version and input
requirements. Legacy `/input/move` can omit metadata and keeps its delta meaning.

### Ownership and failure contract

All calls on a handle are sequential on its creating thread, including destruction.
Wrong-thread calls refuse without changing the run. Do not share buffers or call
concurrently. The caller owns all byte buffers; Rust borrows them only for the
duration of the call. No cross-allocator free operation exists.

Buffers carry explicit byte lengths with no NUL terminator. A null pointer with
capacity zero queries the exact required size. Short buffers get no partial copy
and do not consume output. Required-length pointers must be valid and writable.
One successful output read consumes the whole batch; subsequent reads return
`EMPTY`. Every tick produces a batch, including `[]`; another tick returns
`PENDING_OUTPUT` until the previous batch is consumed. Inspection does not consume
output and may run before or after reads. Invalid output arguments never advance
the application or discard its pending batch.

Each input/configuration is limited to 1 MiB, with at most 4096 queued requests and
16 MiB of input JSON before a tick. Output/inspection JSON is limited to 64 MiB.
The boundary retains at most one output batch. These limits bound boundary-owned
buffers; the application is still responsible for bounding its internal state.

The library must use panic unwinding. Panics never cross the boundary; a caught
driver panic or failed/unencodable tick permanently disables its handle because
state may already have advanced. Only diagnosis and destruction remain available.
An input rejection leaves the handle usable; a custom driver must validate before
mutating its queue. Allocation failure/process abort cannot be recovered by the ABI.
Invalid/dangling memory, aliasing, concurrent calls, double destruction and unloading
the library with live handles remain caller violations.

## Legacy movement surface
- `kitu_init` creates a runtime handle for an embedding host.
- `kitu_submit_move_input` submits one `/input/move` intent into runtime-owned processing.
- `kitu_tick` advances the runtime by one authoritative tick.
- `kitu_pop_render_transform` drains one `/render/player/transform` event for presentation consumers.

This original movement API remains available for compatibility. New complete
applications use the versioned application API above; game rules stay in their
application crate and are executed by `kitu-runtime`.

## Publish readiness
- Status: internal (`publish = false`), but metadata and README prepare the crate for crates.io packaging.
- Run the gate before enabling publication:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Workspace crate overview: `doc/crates-overview.md`
