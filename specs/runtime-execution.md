# Runtime Execution Specification (MVP)

This document defines the runtime-side execution boundary for the current MVP phase.
It complements `specs/runtime-execution-contract.md` by describing the runtime responsibilities and extension boundaries without fixing the internal implementation too early.

## Status

- Normative for execution phase boundaries and timing rules.
- Intentionally non-prescriptive about concrete scheduler, queue, and buffer data structures.
- Compatible with the current architecture in `doc/architecture.md`, where P1 remains partial / frozen.

## Scope

- Authoritative tick-driven execution in `kitu-runtime`.
- Input timing and output visibility rules.
- Extension boundaries for transports, tooling, and future replay.

## Non-goals

- Final implementation of `Runtime::update(dt)`.
- Final internal queue structures for committed/pending inputs.
- Final ECS scheduler details.
- Final replay execution engine.

## Execution ownership

`kitu-runtime` is the only layer allowed to own authoritative simulation execution.

Responsibilities:

- Own the authoritative tick counter.
- Freeze the per-tick committed input batch.
- Dispatch deterministic state update work for the current tick.
- Stage and expose runtime outputs.
- Poll transports only as a delivery source for future ticks.

Non-responsibilities:

- Unity-side prediction or presentation logic.
- Transport-side gameplay routing or state mutation.
- Tool-specific alternate simulation paths.

## Canonical phase order

For tick `N`, runtime behavior follows this stable order:

1. Commit the input batch for tick `N`.
2. Run authoritative state update work for tick `N`.
3. Promote staged outputs for tick `N` to externally visible outputs.
4. Poll transport/input adapters and enqueue messages for tick `N+1`.
5. Advance the tick counter from `N` to `N+1`.

The exact implementation may change later, but this ordering must remain stable unless `doc/architecture.md` is explicitly revised.

## Timing rules

- Inputs received during tick `N` are eligible only for tick `N+1`.
- Outputs produced during tick `N` become externally visible only after the state update phase for tick `N`.
- Transport polling must not directly mutate authoritative world state.
- Tooling input, replay input, and Unity input must enter through the same runtime-owned input path.

## Extension boundaries

Future implementations may add:

- deterministic replay adapters
- authoritative input queue internals
- output drain internals
- TSQ1 or script hooks inside deterministic tick execution

Those additions must preserve the same authority boundary:

- replay supplies ordered inputs, not direct ECS mutation
- transport supplies messages, not gameplay decisions
- Unity stays an input/presentation boundary

## Relationship to other specs

- `specs/runtime-execution-contract.md`: detailed tick contract and API-facing timing rules.
- `specs/transport-envelope.md`: transport-facing envelope shape and metadata rules.
- `specs/osc-addressing.md`: message address namespace and ownership rules.
- `specs/vertical-slice-player-move.md`: first minimal vertical slice built on this execution model.
