# Transport Envelope Specification (Draft MVP)

This document defines the transport-neutral envelope shape for messages crossing Kitu runtime boundaries.
It standardizes metadata needed by runtime, integration tools, and future replay without giving transport layers gameplay responsibility.

## Status

- Draft MVP specification.
- Normative for logical envelope fields and ownership rules.
- Not a final wire-format specification.

## Goals

- Keep `kitu-osc-ir` payloads transport-agnostic.
- Carry enough metadata for deterministic ordering, tracing, and replay artifact generation.
- Avoid embedding gameplay interpretation in transport adapters.

## Envelope model

Each boundary-crossing message is treated as:

1. Envelope metadata
2. OSC-like address
3. ordered argument payload

Conceptual shape:

```text
Envelope {
  message_id,
  direction,
  channel,
  tick_hint?,
  source,
  payload,
}
```

`payload` is an OSC-IR message identified by an address such as `/input/move` or `/render/player/transform`.

## Required logical fields

| Field | Description |
| --- | --- |
| `message_id` | Stable per-message identifier for logs, tracing, and replay artifacts. |
| `direction` | `inbound` or `outbound` relative to `kitu-runtime`. |
| `channel` | Logical origin class such as `unity`, `tool`, `transport`, or `replay`. |
| `source` | Adapter-defined source identifier, for example a client id or tool name. |
| `payload` | Address plus ordered arguments defined by `kitu-osc-ir`. |

## Optional logical fields

| Field | Description |
| --- | --- |
| `tick_hint` | Tick annotation supplied by tooling or replay artifacts. Runtime may validate or ignore it depending on mode. |
| `trace_id` | Correlation id for multi-message diagnostics. |
| `received_at` | Host-side timestamp for observability only; never authoritative for simulation. |
| `schema_version` | Envelope schema version for artifact compatibility. |

## Ownership rules

- Transport adapters may serialize, deserialize, and tag envelopes.
- Transport adapters must not reinterpret `/input/*` or `/render/*` semantics.
- Runtime decides whether an inbound envelope becomes a future committed input.
- Replay tools may persist and restore envelopes, but must not patch world state directly.

## Direction-specific expectations

### Inbound to runtime

- Carries input intents, tooling commands, or replay-fed messages.
- Becomes eligible for authoritative processing only through the runtime input path.
- May include `tick_hint`, but runtime timing rules still apply.

### Outbound from runtime

- Carries render-facing, UI-facing, debug, or diagnostic outputs.
- Reflects already-resolved authoritative state transitions.
- Must be suitable for artifact capture without transport-specific knowledge.

## Serialization policy

- This specification does not lock the final network or FFI wire format.
- MessagePack, JSON, binary FFI structs, or other representations are adapter concerns.
- Any concrete serialization must preserve the logical envelope fields defined here.

## Relationship to replay and integration

- Scenario input files may store envelopes or a reduced equivalent that can be losslessly mapped into envelopes.
- Expected outputs should compare logical envelope content, not transport-specific bytes.
- Summary/report files may reference `message_id`, `channel`, and `tick_hint` for diagnostics.
