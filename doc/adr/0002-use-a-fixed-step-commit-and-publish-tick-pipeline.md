# 0002: Use a fixed-step commit-and-publish tick pipeline

- Status: Accepted
- Decision date: 2026-03-08

## Context

Runtime results must not depend on when a transport happens to be polled during
a host frame. Inputs can arrive from FFI, local channels, network adapters, or
replay, and tools need an exact tick on which each input becomes authoritative
and each output becomes visible. A variable-step loop or immediate transport
mutation would make ordering and replay depend on host timing.

## Decision

Run authoritative simulation at a configured fixed tick rate. `update(dt)` is
only an accumulator that executes zero or more complete `tick_once()` calls.
Reject non-finite, negative, or unrepresentable deltas.

For tick `N`, preserve this phase order:

1. commit the pending FIFO input queue as the immutable batch for `N`;
2. validate and collect runtime-owned boundary inputs;
3. dispatch ECS systems deterministically;
4. apply runtime-owned slice updates and stage outputs;
5. publish staged outputs to the externally visible FIFO buffer;
6. drain transport messages into the pending queue for `N+1`; and
7. increment the monotonic tick as the final phase.

Inputs observed while tick `N` executes are never applied during `N`. A failed
validation must not partially apply that input batch or advance the tick.

## Consequences

- Simulation and replay observe the same input commit and output visibility
  barriers regardless of transport timing.
- Rendering may interpolate externally, but wall-clock jitter does not change
  authoritative step size.
- Every new scripting, timeline, reload, or tooling hook needs an explicit safe
  phase instead of mutating state asynchronously.
- Hosts may see one-tick input latency, and large host deltas may execute
  multiple ticks before returning.
- FIFO queues and failure behavior are part of the runtime contract and require
  ordering and atomicity tests.

## Alternatives considered

- **Variable-step simulation**: simpler host integration, but results vary with
  frame rate and make replay harder to compare.
- **Apply transport messages immediately**: lowers latency but makes state
  depend on poll timing within a tick.
- **Poll transport before current-tick dispatch**: can apply fresh inputs
  sooner, but removes the stable `N` receive to `N+1` apply boundary.
- **Expose staged outputs immediately**: allows re-entrant observation of
  partial tick results.

## References

- [Runtime execution contract](../specs/runtime-execution-contract.md)
- [`kitu-runtime`](../../crates/kitu-runtime/README.md)
- Pull request [#22](https://github.com/Nagitch/kitu-logic-processor/pull/22)
