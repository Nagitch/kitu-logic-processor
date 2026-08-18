# 0001: Keep authoritative gameplay in the Rust runtime

- Status: Accepted
- Decision date: 2026-04-05

## Context

Kitu must support a Unity-embedded single-player process, a standalone server,
headless tests, replay tools, and browser administration without implementing
game rules separately in every host. Unity is effective for presentation and
device input, but tying authority to MonoBehaviours would make server, replay,
and embedded behavior diverge. Letting tooling mutate its own copy of world
state would create the same split.

## Decision

Make `kitu-runtime` and its Rust-owned ECS state authoritative for gameplay,
tick progression, and world state. Use the same runtime implementation in
standalone hosts, the demo application, replay tools, and the Unity embedding.

Treat Unity and other clients as input and presentation boundaries. They submit
intent through runtime-owned input queues and consume runtime output or state
projections. Keep `kitu-unity-ffi` as a narrow translation adapter: it creates a
runtime, submits inputs, advances ticks, and drains presentation events; it does
not own gameplay rules. Shell and Web Admin operations that mutate state must
likewise enter runtime-owned APIs or message paths.

## Consequences

- Equal runtime state and ordered inputs can exercise the same logic in Unity,
  servers, CI, and replay.
- Unity scenes, browser state, and operator tools cannot be the authoritative
  source for gameplay outcomes.
- Hosts must translate local input and render the resulting projections, which
  can add latency compared with direct client-side mutation.
- Client prediction may be added later only as explicitly non-authoritative
  behavior with reconciliation rules.
- ABI, network, and tool adapters stay small but must preserve runtime timing
  and validation semantics.

## Alternatives considered

- **Put gameplay in Unity**: gives direct access to presentation objects but
  duplicates or excludes headless and server behavior.
- **Maintain separate embedded and server runtimes**: permits specialization
  but makes deterministic equivalence an ongoing integration problem.
- **Let tools patch ECS state directly**: convenient for debugging, but bypasses
  validation, event ordering, and replayable intent.

## References

- [Kitu architecture](../architecture.md)
- [Player-move runtime/host boundary](../specs/vertical-slice-player-move.md)
- [`kitu-unity-ffi` boundary](../../crates/kitu-unity-ffi/README.md)
- Pull requests [#19](https://github.com/Nagitch/kitu-logic-processor/pull/19) and
  [#32](https://github.com/Nagitch/kitu-logic-processor/pull/32)
