# 0003: Keep OSC-IR semantics independent of wire transport

- Status: Accepted
- Decision date: 2026-06-23

## Context

Runtime messages cross local channels, a C ABI, WebSocket JSON, WebTransport,
tests, and replay artifacts. If address and argument semantics are owned by a
wire protocol, changing transport can change gameplay behavior. Conversely,
gateways and brokers need routing and correlation metadata that should not be
inserted into OSC addresses or payload bytes.

## Decision

Use `kitu-osc-ir` messages and bundles as the logical application event model.
OSC-style addresses are API contracts, not network routes: `/input/*` carries
intent into the runtime, while `/render/*` and `/ui/*` carry authoritative
projections outward. Keep gameplay interpretation out of transport adapters.

Let each boundary serialize the same logical model as appropriate: native
calls, JSON, OSC packet bytes, or another adapter representation. For network
paths that need metadata, wrap application payload bytes in the MessagePack
Kitu Envelope Protocol (KEP). Keep KEP route, correlation, and flags separate
from the unchanged OSC payload. Frame KEP according to transport—one envelope
per WebSocket message or datagram, and big-endian length-prefixed envelopes on
WebTransport streams.

Only claim support for the OSC argument tags and packet shapes implemented by
the shared codec. Expand that subset when a concrete runtime or client path
requires it.

## Consequences

- Runtime, Unity, Web Admin, gateway, and replay can share address semantics
  without depending on one network stack.
- Gateways may route and correlate envelopes without rewriting gameplay
  payloads.
- Multiple serializers must be tested against the same logical messages, and
  unsupported OSC shapes must fail explicitly.
- KEP metadata is not authoritative simulation time; transport timestamps and
  routes cannot bypass the runtime input contract.
- Application address versioning remains separate from transport endpoint
  naming.

## Alternatives considered

- **Use WebSocket JSON as the canonical model**: easy for the browser, but
  couples the runtime contract to one transport and language representation.
- **Put routing into OSC address prefixes**: avoids an envelope but mixes
  infrastructure topology with application APIs.
- **Use KEP fields as gameplay commands**: makes transport adapters interpret
  domain semantics and weakens replay portability.
- **Adopt every OSC type immediately**: broad compatibility, but expands codec
  and validation scope before Kitu has consumers for it.

## References

- [OSC address ownership](../specs/osc-addressing.md)
- [Kitu Envelope Protocol](../specs/kitu-envelope-protocol.md)
- [`kitu-osc-ir`](../../crates/kitu-osc-ir/README.md)
- [`kitu-transport`](../../crates/kitu-transport/README.md)
- Pull request [#87](https://github.com/Nagitch/kitu-logic-processor/pull/87)
