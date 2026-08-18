# 0004: Keep WebTransport behind an experimental edge gateway

- Status: Accepted
- Decision date: 2026-06-27

## Context

WebTransport offers browser-accessible HTTP/3 streams and datagrams, but the
MVP already has stable, ordered WebSocket paths for Web Admin, Unity, local
integration tests, and authoritative replay validation. Replacing those paths
would enlarge the runtime and TLS blast radius before WebTransport behavior and
production requirements are established. Unreliable datagrams also cannot
satisfy deterministic command ordering.

## Decision

Keep the existing application server authoritative for OSC handling and
application logic. Terminate WebTransport in a separate local-development edge
gateway. Validate KEP there and relay it to the existing application WebSocket
endpoint as binary KEP messages over the internal network. Reuse one serialized
internal relay per WebTransport session and preserve ordered response frames.

Retain WebSocket as the MVP Unity/runtime transport and as the browser fallback.
Send authoritative OSC actions and persistent mutations only over reliable
streams or WebSocket. Restrict WebTransport datagrams to small, loss-tolerant
JSON probes, telemetry, or replaceable preview state. Do not retry a request on
WebSocket after its WebTransport write may have succeeded, because duplicate
mutations are worse than an explicit failure.

## Consequences

- WebTransport can be evaluated without making the runtime or application host
  depend on QUIC/HTTP3 and browser certificate behavior.
- Existing WebSocket clients, tests, and fallback remain valid comparison paths.
- The gateway adds a hop, session relay state, TLS setup, and Docker integration
  work.
- Datagram users must tolerate loss, duplication, and reordering; they cannot
  rely on replay or acknowledgement semantics.
- Promoting WebTransport to an authoritative runtime transport requires a new
  decision with ordering, delivery, deployment, and compatibility evidence.

## Alternatives considered

- **Replace WebSocket with WebTransport immediately**: removes fallback
  duplication but broadens migration and reliability risk.
- **Embed WebTransport in `kitu-runtime`**: reduces a hop but couples simulation
  orchestration to one transport and TLS stack.
- **Add a new internal TCP or gRPC service**: cleaner separation for a mature
  gateway, but creates a new app-server protocol before the experiment needs it.
- **Use datagrams for OSC commands**: lower overhead, but loss and reordering
  violate authoritative action requirements.

## References

- [WebTransport gateway experiment](../specs/webtransport-gateway.md)
- [KEP transport mappings](../specs/kitu-envelope-protocol.md#transport-mapping)
- Pull requests [#96](https://github.com/Nagitch/kitu-logic-processor/pull/96)
  and [#98](https://github.com/Nagitch/kitu-logic-processor/pull/98)
