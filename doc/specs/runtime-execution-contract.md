# Runtime Execution Contract (MVP)

This document defines the authoritative runtime loop contract for `kitu-runtime`.
It is the normative specification for tick order, input timing, transport polling, and output emission.

## Scope

- Authoritative simulation loop in Rust (`kitu-runtime`).
- Unity remains presentation + input boundary only.
- Transport remains delivery-only and must not own gameplay logic.

## Fixed timestep and `update(dt)`

- `RuntimeConfig.tick_rate_hz` defines the fixed simulation step (`frame_time`).
- `Runtime::update(dt)` accumulates wall-clock delta and executes `tick_once()` while `accumulator >= frame_time`.
- non-finite `dt` (`NaN`, `+/-∞`) is invalid input.
- `dt < 0` is invalid input.
- `dt` that exceeds `Duration::MAX` is invalid input.
- `tick_rate_hz` must be non-zero and produce a positive non-zero `frame_time`.

Pseudo flow:

1. `accumulator += dt`
2. While `accumulator >= frame_time`:
   1. Execute one authoritative tick via `tick_once()`
   2. `accumulator -= frame_time`

## Tick contract

For tick `N`, execution order is fixed as follows:

1. **Commit input batch for tick `N`**
   - Clear previous committed inputs, then move `pending_inputs` into `committed_inputs` for tick `N`.
2. **Collect runtime-boundary inputs for tick `N`**
   - Validate and snapshot committed messages that the current runtime owns directly.
   - Current MVP behavior collects `/input/move` before ECS dispatch so invalid movement input fails the tick before state mutation.
   - A persistent `RuntimeApplication` validates the frozen metadata-bearing batch before any mutation.
3. **Dispatch ECS systems for tick `N`**
   - Run scheduled ECS systems in deterministic order.
4. **Apply runtime-owned MVP slice updates**
   - Current MVP behavior applies collected `/input/move` intents after ECS dispatch and stages `/render/player/transform`.
5. **Update the installed application**
   - Invoke its persistent tick hook using the fixed timestep and committed inputs.
   - Application state lives in typed `EcsWorld` resources. State-dependent rejections are outputs, not failed ticks.
6. **Emit outputs for tick `N`**
   - Move staged outputs into externally visible `output_buffer`.
7. **Poll transport for next tick input**
   - Drain `poll_event()` until empty.
   - Any received `TransportEvent::Message` is enqueued into `pending_inputs`.
8. **Advance tick**
   - `tick = tick.next()`.

## Input timing rule (normative)

Inputs received during tick `N` are never applied during tick `N`.
They are queued in `pending_inputs` and become the committed input batch at tick `N+1`.

This rule is mandatory for deterministic replay and transport-timing independence.

## Output timing rule

Outputs generated during tick `N` are staged during execution and only become externally visible in the output buffer at the output emission phase of tick `N`.
Hosts should poll outputs after `update()`/`tick_once()` returns.

## Minimal API surface (MVP)

- `update(dt: f32) -> Result<u32>`: advance fixed ticks from an accumulator.
- `tick_once() -> Result<()>`: execute exactly one authoritative tick with the fixed phase order.
- `enqueue_input(bundle)`: queue host-provided input for a future tick.
- `queue_output(bundle)`: stage runtime outputs for the output emission phase.
- `drain_output_buffer()`: read emitted outputs in FIFO order.
- `drain_committed_inputs()`: consume the committed input batch in FIFO order.

## Relationship to architecture docs

- `doc/architecture.md` defines architecture-level invariants.
- `doc/detailed-flows.md` uses this tick contract when describing UC-02 and runtime boundaries.

## Persistent applications and host scheduling

`install_application` installs one `RuntimeApplication` before tick zero. Its
`validate_inputs` hook must be read-only, while `tick` must be infallible after
validation. The `snapshot` hook returns detached logical OSC projections without
advancing time. `EcsWorld::{insert_resource, resource, resource_mut}` stores typed,
world-owned application state independently from the one-shot system scheduler.

`RuntimeInput` preserves `InputMetadata` (producer, message ID, schema) and a
runtime-assigned monotonically increasing enqueue `sequence`. Hosts should call
`try_enqueue_input`: it validates structural payloads before admission and returns
the sequence. This prevents a malformed network packet from discarding other
accepted inputs. Unchecked enqueue methods remain available for legacy callers;
the tick still rejects a malformed committed batch atomically before dispatch.

The demo admin host owns a Tokio interval at 60 Hz, independent of websocket and
HTTP handlers. Those handlers enqueue inputs without invoking `tick_once`. The
clock drains and broadcasts outputs after each barrier. No-input ticks continue
running; paused applications still process control input and inspection while
freezing their own game clocks. Generic legacy app actions retain their existing
immediate world-object operations; they cannot mutate Arena resources. Runtime
restart creates a new session and is not a resume-from-disk operation.
