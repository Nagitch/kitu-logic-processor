# Vertical Slice Specification: Player Move (P2 MVP)

This document defines the minimum vertical slice contract for player movement without requiring the full end-to-end runtime implementation yet.
It covers:

- `/input/move`
- authoritative state update
- `/render/player/transform`

## Status

- Normative for boundary contracts and responsibility separation.
- Intentionally leaves runtime internals and full ECS implementation open.
- Depends on `doc/architecture.md` and the runtime timing rules in `specs/runtime-execution-contract.md`.

## Slice purpose

The slice exists to validate the smallest authoritative path:

1. a host sends movement intent
2. runtime commits that intent on the next tick
3. authoritative state updates player transform
4. runtime emits a render-facing transform message

This is a contract-first slice, not the full implementation milestone.

## Responsibility split

### Unity / host boundary

Owns:

- device input sampling
- conversion from local controls to movement intent
- transport or FFI submission of `/input/move`
- application of `/render/player/transform` to presentation objects

Does not own:

- simulation authority
- movement resolution
- authoritative position state

### `kitu-runtime`

Owns:

- input batch timing
- authoritative tick sequencing
- handoff into deterministic state update work
- output staging and visibility timing

Does not own:

- transport serialization details
- Unity presentation logic

### State update layer

Owns:

- interpretation of movement intent into authoritative movement state
- mutation of authoritative player transform/state for the current tick
- production of render-facing output data

Does not own:

- direct device input polling
- transport concerns

## Message contracts

### Inbound: `/input/move`

Purpose:

- express desired movement direction/intensity from a boundary client to the authoritative runtime

Producer:

- Unity input bridge, shell/tooling adapter, or replay adapter

Consumer:

- runtime-owned input intake, then deterministic state update work

Logical shape:

```json
{
  "address": "/input/move",
  "args": {
    "entity_id": "player:local",
    "x": 0.0,
    "y": 1.0
  }
}
```

Field notes:

| Field | Type | Notes |
| --- | --- | --- |
| `entity_id` | string | Required logical target. Exact identifier scheme may evolve. |
| `x` | number | Horizontal intent component, nominal range `[-1.0, 1.0]`. |
| `y` | number | Vertical intent component, nominal range `[-1.0, 1.0]`. |

Contract notes:

- Represents intent only, not position delta.
- Runtime may normalize, clamp, or reject invalid values later; that policy is not fixed here.
- Arrival during tick `N` makes it eligible for tick `N+1`.

### Internal authoritative state update

Purpose:

- convert committed movement intent into authoritative player transform/state

Minimal input to the update stage:

```json
{
  "tick": 120,
  "entity_id": "player:local",
  "move_intent": {
    "x": 0.0,
    "y": 1.0
  },
  "previous_transform": {
    "position": { "x": 10.0, "y": 4.0, "z": 0.0 }
  }
}
```

Minimal output from the update stage:

```json
{
  "tick": 120,
  "entity_id": "player:local",
  "transform": {
    "position": { "x": 10.0, "y": 5.0, "z": 0.0 }
  }
}
```

Contract notes:

- The state update layer consumes committed input, not transport events directly.
- The exact component layout, scheduler order within the state update phase, and movement math remain open.
- Collision, terrain, animation state, and prediction are out of scope for this minimum slice.

### Outbound: `/render/player/transform`

Purpose:

- expose authoritative transform data to presentation consumers

Producer:

- runtime-owned output staging after authoritative state update

Consumer:

- Unity presentation layer or replay/report tooling

Logical shape:

```json
{
  "address": "/render/player/transform",
  "args": {
    "entity_id": "player:local",
    "tick": 120,
    "position": { "x": 10.0, "y": 5.0, "z": 0.0 }
  }
}
```

Field notes:

| Field | Type | Notes |
| --- | --- | --- |
| `entity_id` | string | Required stable presentation binding key. |
| `tick` | integer | Authoritative tick that produced this transform. |
| `position` | object | Minimum position payload for the MVP slice. |

Contract notes:

- Runtime may later add rotation, velocity, or interpolation hints in a backward-compatible evolution.
- Presentation consumers must treat this as authoritative output, not a request.

## End-to-end timing model

For a message received during tick `N`:

1. `/input/move` arrives through FFI, transport, or replay adapter.
2. Runtime stores it as pending input for the next eligible tick.
3. At tick `N+1`, runtime commits the input batch.
4. State update work consumes the committed movement intent.
5. Runtime stages `/render/player/transform`.
6. The output becomes externally visible before transport polling for the following tick completes.

## Deferred decisions

- exact numeric normalization rules for `x` / `y`
- authoritative entity id registry format
- transform coordinate handedness details per host
- internal ECS components and system ordering within the state update phase
- whether render output carries full transform or partial deltas
