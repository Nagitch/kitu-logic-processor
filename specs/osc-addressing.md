# OSC Addressing Specification (Draft MVP)

This document defines the first address namespace rules for Kitu OSC-like messages.
It establishes ownership boundaries for address families while leaving room for future gameplay expansion.

## Status

- Draft MVP specification.
- Normative for address family ownership and naming conventions.
- Not a complete list of all future addresses.

## Design rules

- Addresses are logical API contracts, not transport routes.
- Each address belongs to one responsibility boundary.
- Unity stays at the presentation/input boundary.
- Transport stays delivery-only and does not own gameplay semantics.

## Naming conventions

- Use absolute OSC-style paths beginning with `/`.
- Use lowercase kebab-free segments unless a future schema requires otherwise.
- Reserve the first segment for a top-level domain of responsibility.
- Prefer stable nouns and verbs over device-specific terminology.

Examples:

- `/input/move`
- `/render/player/transform`
- `/debug/log`

## Address families

### `/input/*`

Purpose:

- Host-originated gameplay or operator intent entering the authoritative runtime.

Rules:

- Produced by Unity, tools, or replay adapters.
- Consumed by `kitu-runtime` input intake.
- Must describe intent, not post-simulation state.

Examples:

- `/input/move`
- `/input/attack`

### `/render/*`

Purpose:

- Runtime-originated presentation data for Unity or other presentation consumers.

Rules:

- Produced only after authoritative state update.
- Must not be interpreted as authoritative input.
- Should describe presentation-relevant state snapshots or events.

Examples:

- `/render/player/transform`
- `/render/enemy/spawn`

### `/ui/*`

Purpose:

- Runtime-originated user interface updates.

Rules:

- Separate from world render output when the consumer is UI-specific.
- Still derives from authoritative state, never from client-side guesswork.

### `/debug/*`

Purpose:

- Tooling and diagnostics across shell, admin, and test runners.

Rules:

- May enter runtime as commands or leave runtime as diagnostics.
- Must still respect the same runtime execution path when mutating state.

### `/game/*`

Purpose:

- Internal or downstream domain events that are not pure presentation.

Rules:

- Intended for runtime-side composition, tooling visibility, or future integrations.
- Not a substitute for direct ECS mutation from outside runtime.

## Reserved constraints

- Transport-specific prefixes are disallowed in the logical address namespace.
- Unity object names, scene names, or network socket identifiers must not appear as address roots.
- Address versioning, if needed later, should be handled by envelope/schema versioning before path proliferation.

## Initial required addresses

The current MVP planning requires these addresses to remain reserved and documented:

- `/input/move`
- `/render/player/transform`

Their concrete message shapes are defined in `specs/vertical-slice-player-move.md`.
