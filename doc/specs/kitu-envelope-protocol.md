# Kitu Envelope Protocol (KEP)

## Overview

KEP (Kitu Envelope Protocol) is a transport-independent envelope format used within the Kitu ecosystem.

KEP provides a lightweight metadata layer around application payloads while remaining independent from the underlying transport protocol.

Supported transports may include:

- WebSocket
- WebTransport
- QUIC
- TCP
- IPC

Payloads are transport-agnostic and may contain OSC packets or other application-defined formats.

---

# Design Goals

- Transport independence
- Minimal overhead
- Binary efficiency
- Forward compatibility
- OSC interoperability
- Flexible metadata support

---

# Serialization

KEP envelopes are encoded using MessagePack.

The top-level structure is a MessagePack map.

Example:

```json
{
  "t": "osc",
  "r": "/room/main",
  "i": 42,
  "f": 0,
  "p": "<binary>"
}
```

---

# Envelope Fields

| Key | Type | Required | Description |
|------|------|------|------|
| t | string | Yes | Payload type |
| r | string | No | Route identifier |
| i | uint64 | No | Correlation identifier |
| f | uint64 | No | Flags |
| p | binary | Yes | Payload data |

Unknown fields should be ignored.

Applications may introduce additional fields as needed.

---

# Payload Type (`t`)

Identifies the payload format.

Examples:

```text
osc
json
msgpack
tsq1
```

Payload type names are application-defined.

---

# Route (`r`)

Identifies the logical destination of a message.

Examples:

```text
/room/main
/room/123
/chat/global
/user/456
```

Routing semantics are implementation-defined.

The route field is separate from OSC addresses and may be used by gateways, brokers, or distributed server architectures.

---

# Correlation Identifier (`i`)

Used to associate related messages.

Examples:

- Request / response matching
- Acknowledgements
- Tracing
- Diagnostics

Example:

```text
1001
```

---

# Flags (`f`)

Bitmask used for implementation-specific behaviors.

Suggested flag assignments:

| Bit | Meaning |
|------|------|
| 0 | Reliable |
| 1 | Compressed |
| 2 | Encrypted Payload |
| 3-63 | Reserved |

Applications may define additional meanings.

---

# Payload (`p`)

Contains the serialized payload.

For OSC messages:

```text
p = OSC Packet Binary
```

The OSC payload remains unmodified and follows standard OSC encoding rules.

For JSON messages:

```text
p = UTF-8 JSON bytes
```

The JSON payload schema is application-defined. For example, a gateway may use
`t = "json"` with `r = "/server/event"` to carry server event JSON without
changing the transport-level envelope.

---

# OSC Payload Example

OSC packet:

```text
Address:
/avatar/pose

Arguments:
[
  1.0,
  2.0,
  3.0
]
```

KEP envelope:

```json
{
  "t": "osc",
  "r": "/room/main",
  "i": 1001,
  "f": 0,
  "p": "<osc binary>"
}
```

---

# Transport Mapping

## WebSocket

One KEP envelope per WebSocket message.

```text
WebSocket Message
    ↓
KEP Envelope
```

---

## WebTransport Stream

One KEP envelope per stream message.

```text
WebTransport Stream
    ↓
KEP Envelope
```

---

## WebTransport Datagram

One KEP envelope per datagram.

```text
WebTransport Datagram
    ↓
KEP Envelope
```

Datagrams should remain within the practical MTU limits of the transport path.

---

# Architecture Example

```text
Transport Layer
    ↓
KEP Envelope
    ↓
Application Payload
```

Example:

```text
WebTransport
    ↓
KEP
    ↓
OSC
```

or

```text
WebSocket
    ↓
KEP
    ↓
TSQ1
```

---

# Notes

KEP is intentionally lightweight.

Its primary purpose is to provide routing, correlation, and metadata capabilities without modifying application payload formats such as OSC.

Payload formats remain independently defined and may evolve without requiring changes to the KEP envelope structure.
