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
3. **Dispatch ECS systems for tick `N`**
   - Run scheduled ECS systems in deterministic order.
4. **Apply runtime-owned MVP slice updates**
   - Current MVP behavior applies collected `/input/move` intents after ECS dispatch and stages `/render/player/transform`.
5. **Emit outputs for tick `N`**
   - Move staged outputs into externally visible `output_buffer`.
6. **Poll transport for next tick input**
   - Drain `poll_event()` until empty.
   - Any received `TransportEvent::Message` is enqueued into `pending_inputs`.
7. **Advance tick**
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
