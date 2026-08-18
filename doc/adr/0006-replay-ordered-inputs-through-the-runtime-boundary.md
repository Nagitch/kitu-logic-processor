# 0006: Replay ordered inputs through the runtime boundary

- Status: Accepted
- Decision date: 2026-06-16

## Context

Deterministic testing and debugging need to reproduce behavior outside Unity
and without a live network. Capturing final ECS snapshots alone cannot explain
how state was reached, and directly patching components during replay bypasses
the validation, queue timing, and systems used in production. Comparing wire
bytes would also bind fixtures to one transport serialization.

## Decision

Define replay scenarios as a schema-versioned initial context plus ordered,
tick-indexed inbound logical messages. Feed those messages through the same
runtime `enqueue_input` and `tick_once` boundary used by live adapters. Do not
patch ECS or runtime-owned state directly.

Capture and compare logical outbound messages after the runtime output barrier,
not concrete WebSocket, KEP, JSON, or FFI bytes. Store checked-in
`scenario.json` and `expected.json` contracts; write generated summaries and
future event/diff reports outside the scenario tree. Exclude wall-clock time
from pass/fail semantics and permit deterministic sentinel timestamps in smoke
summaries.

## Consequences

- Replay validates runtime authority, tick timing, parsing, state mutation, and
  outputs together rather than simulating their effects.
- The same scenario can be driven by a local runner, CI, or a future transport
  adapter without changing expected logical behavior.
- Fixtures must declare stable fields and avoid depending on incidental wire or
  wall-clock details.
- Reproducing nondeterministic external systems requires them to be modeled as
  ordered boundary inputs or controlled content versions.
- The current smoke runner proves the player-move path; broader replay coverage
  and richer reports remain incremental work under the same contract.

## Alternatives considered

- **Snapshot and restore ECS state directly**: fast for tests, but bypasses the
  production input path and can encode internal component layout.
- **Record network packets byte-for-byte**: exact for one adapter, but fragile
  across serialization changes and unsuitable for FFI/local-channel runs.
- **Use wall-clock timestamps to schedule replay**: mirrors capture timing but
  reintroduces host jitter and slows deterministic tests.

## References

- [Integration/replay framework](../specs/integration-replay-framework.md)
- [Integration runner](../../kitu-integration-runner/README.md)
- [Replay runner](../../tools/README.md)
- Pull request [#62](https://github.com/Nagitch/kitu-logic-processor/pull/62)
