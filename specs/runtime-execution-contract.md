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
   - Append `pending_inputs` into `committed_inputs` (preserving any previously committed, not-yet-consumed inputs).
2. **Dispatch ECS systems for tick `N`**
   - Authoritative state update phase.
3. **Emit outputs for tick `N`**
   - Move staged outputs into externally visible `output_buffer`.
4. **Poll transport**
   - Drain `poll_event()` until empty.
   - Any received `TransportEvent::Message` is enqueued into `pending_inputs`.
5. **Advance tick**
   - `tick = tick.next()`.

## Input timing rule (normative)

Inputs received during tick `N` are never applied during tick `N`.
They are queued in `pending_inputs` and become the committed input batch at tick `N+1`.

This rule is mandatory for deterministic replay and transport-timing independence.

## Output timing rule

Outputs generated during tick `N` are staged during execution and only become externally visible in the output buffer at phase 3 of tick `N`.
Hosts should poll outputs after `update()`/`tick_once()` returns.

## Minimal API surface (MVP)

- `update(dt: f32) -> Result<u32>`: advance fixed ticks from an accumulator.
- `tick_once() -> Result<()>`: execute exactly one authoritative tick with the fixed phase order.
- `enqueue_input(bundle)`: queue host-provided input for a future tick.
- `queue_output(bundle)`: stage runtime outputs for emission at phase 3.
- `drain_output_buffer()`: read emitted outputs in FIFO order.
- `drain_committed_inputs()`: consume the committed input batch in FIFO order.

## Relationship to architecture docs

- `doc/architecture.md` defines architecture-level invariants.
- `doc/detailed-flows.md` uses this tick contract when describing UC-02 and runtime boundaries.
