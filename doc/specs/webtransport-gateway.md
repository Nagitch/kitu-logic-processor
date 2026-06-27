# WebTransport Gateway Experiment

## Status

Experimental local-development implementation.

The existing application server remains the authority for OSC handling and application logic. The gateway terminates WebTransport and relays validated KEP envelopes to the existing WebSocket endpoint as binary frames over the Docker internal network.

## Current WebSocket and OSC dependency points

- `apps/demo-game/src/bin/admin_host.rs`
  - `/ws`: Web Admin JSON OSC-IR WebSocket endpoint.
  - `/ws/runtime`: Unity/runtime JSON OSC-IR WebSocket endpoint.
  - `ClientOscMessage`, `JsonOscArg`, `handle_client_osc`, and `handle_runtime_osc` convert JSON OSC-IR messages into `kitu_osc_ir::OscMessage`.
- `tools/kitu-web-admin/frontend/src/lib/admin-client.ts`
  - Connects to `PUBLIC_KITU_ADMIN_WS_URL` or `ws://localhost:8787/ws`.
  - Receives JSON `ServerEvent` messages and sends JSON OSC messages.
- `kitu-integration-runner/unity-demo-game/.../KituNetworkRuntimeClient.cs`
  - Connects to `ws://127.0.0.1:8787/ws/runtime`.
  - Sends and receives the current JSON OSC-IR shape.

The WebSocket endpoints are intentionally retained as fallback and comparison paths.

## Adopted shape

```text
Browser Client
  -> WebTransport / KEP / OSC packet binary
WebTransport Gateway Container
  <-> Docker internal WebSocket binary KEP relay
Existing Application Server Container
```

## Why `wtransport`

The gateway uses the Rust `wtransport` crate because it provides a high-level WebTransport-over-HTTP/3 server API, supports QUIC streams and datagrams, and includes development self-signed identity helpers. The current 0.7.x crate requires Rust 1.88, so the gateway is excluded from the root Rust 1.82 workspace and owns `tools/kitu-webtransport-gateway/rust-toolchain.toml`.

The main workspace remains pinned to Rust 1.82 for existing crates and CI.
Gateway development and compose execution use `tools/kitu-webtransport-gateway/Dockerfile`, which is based on `rust:1.88-bookworm`.

## Internal protocol choice

Candidates considered:

| Option | Impact | Notes |
| --- | --- | --- |
| Internal WebSocket with binary KEP frames | Low | Reuses `/ws`, preserves existing text/JSON behavior, and avoids new server endpoints. |
| TCP | Medium | Requires a new listener/protocol in the app server. |
| HTTP | Medium | Simple request path, but does not match bidirectional runtime updates well. |
| gRPC | High | Adds protobuf/service tooling before the protocol is stable. |
| MessagePack over TCP | Medium | Good fit later, but still requires a new app-server listener. |

The first implementation uses internal WebSocket because it has the smallest blast radius. Text frames retain the existing JSON OSC-IR shape for browser and Unity fallback clients. Binary frames carry one KEP envelope per WebSocket message.

The client-to-server path uses `t = "osc"` and `p = OSC packet binary`. The gateway validates the KEP envelope and forwards the original MessagePack bytes to `ws://demo-game:8787/ws`; the existing application server decodes KEP, extracts the OSC packet binary, and runs the same OSC handling path used by JSON clients.

The server-to-client path uses `t = "json"` and `p = ServerEvent JSON bytes` with route `/server/event`. This keeps the current `ServerEvent` shape intact while making the transport hop KEP-native in both directions. The WebTransport gateway relays the first KEP JSON response from the internal WebSocket back on the WebTransport response stream.

## Docker Compose

Both local compose files include the gateway:

```sh
docker compose -f tools/kitu-web-admin/docker-compose.yml up --build
```

or:

```sh
docker compose -f apps/demo-game/docker-compose.yml up --build
```

Services:

- `demo-game`: existing app server on `http://localhost:8787`.
- `webtransport-gateway`: WebTransport gateway on UDP `9443`.
- `frontend` or `web-admin`: Web Admin on `http://localhost:5173`.

The frontend receives:

- `PUBLIC_KITU_ADMIN_WS_URL=ws://localhost:8787/ws`
- `PUBLIC_KITU_ADMIN_WT_URL=https://localhost:9443`
- `PUBLIC_KITU_ADMIN_KEP_ROUTE=/room/main`

Gateway-only validation can be run from the repository root:

```sh
tools/kitu-webtransport-gateway/scripts/check-in-docker.sh
```

Gateway smoke validation can be run from the repository root:

```sh
tools/kitu-webtransport-gateway/scripts/smoke-in-docker.sh
```

The smoke script starts `demo-game` and `webtransport-gateway`, sends one KEP
`osc` envelope over a WebTransport bidirectional stream, verifies that a KEP
`json` response envelope is returned on the response stream, and checks that the
existing application server state contains the spawned `webtransport-smoke`
object.

## TLS notes

WebTransport requires TLS because it runs over HTTP/3 and QUIC.

The gateway can either:

- load a development certificate with `KITU_WT_GATEWAY_CERT` and `KITU_WT_GATEWAY_KEY`, or
- generate an ephemeral self-signed certificate for local experiments.

Browser verification is the hard part. The local development path uses the
WebTransport `serverCertificateHashes` option so the browser can authenticate a
short-lived self-signed certificate without modifying the host OS trust store.

Generate the local certificate files before starting Compose:

```sh
tools/kitu-webtransport-gateway/scripts/generate-dev-cert-in-docker.sh
```

The script writes ignored files under `tools/kitu-webtransport-gateway/certs/`:

- `webtransport-cert.pem`: ECDSA P-256 self-signed certificate for `localhost`, valid for 13 days.
- `webtransport-key.pem`: private key used by the gateway.
- `webtransport.env`: Compose env file containing `KITU_WT_GATEWAY_CERT`, `KITU_WT_GATEWAY_KEY`, and `PUBLIC_KITU_ADMIN_WT_CERT_SHA256`.
- `webtransport-cert.sha256`: SHA-256 hash of the DER certificate.

Both Compose files load `webtransport.env` if it exists. If it does not exist,
the gateway falls back to an ephemeral self-signed certificate, which is useful
for server bring-up but most browsers will reject it.

An OS-trusted local CA, for example through `mkcert`, is also possible, but it
requires changing the developer machine trust store and is intentionally left as
an explicit manual step rather than Compose startup behavior.

## Browser behavior

The Web Admin now attempts WebTransport only when `PUBLIC_KITU_ADMIN_WT_URL` is set and the browser exposes `window.WebTransport`.

WebSocket remains active for:

- connection state,
- `ServerEvent` broadcasts,
- state snapshots,
- logs,
- fallback OSC sends.

For OSC sends, the browser tries WebTransport first after the WebTransport session is ready. If that send fails, it falls back to the existing WebSocket JSON send path.
When WebTransport succeeds, the browser reads a KEP `json` response envelope from the response stream and applies the decoded `ServerEvent`.

The Web Admin header exposes three independent operational statuses:

- `WS`: the existing WebSocket connection state.
- `WT`: WebTransport readiness (`disabled`, `unsupported`, `connecting`, `ready`, `closed`, or `error`).
- `OSC`: the most recent OSC send path (`wt`, `ws fallback`, `ws`, or `none`).

Safe WebTransport failures before the request is written are shown as
`OSC ws fallback` and retried through the existing WebSocket JSON path.
Failures after the WebTransport request is written are shown as failed
WebTransport sends and are not retried over WebSocket.

## Implemented KEP support

`crates/kitu-transport` now contains:

- `KepEnvelope`
- `encode_kep_envelope`
- `decode_kep_envelope`
- `encode_osc_packet`
- `decode_osc_packet`

Supported OSC packet argument types:

- `i`: int32
- `h`: int64
- `f`: float32
- `s`: string
- `T` / `F`: bool

`apps/demo-game/src/bin/admin_host.rs` accepts KEP on the existing WebSocket endpoints:

- Text frames: existing JSON OSC-IR messages.
- Binary frames: KEP MessagePack envelopes with `t = "osc"` and `p = OSC packet binary`.

After a connection sends a binary KEP request, subsequent server events on that
connection are sent as KEP binary frames with `t = "json"` and
`r = "/server/event"`.

## Browser connection check

1. Start the compose stack.
   For browser WebTransport testing, generate the local certificate first:

```sh
tools/kitu-webtransport-gateway/scripts/generate-dev-cert-in-docker.sh
```

2. Open `http://localhost:5173`.
3. Confirm the WebSocket status remains open.
4. Trigger an OSC action from a UI path that calls `sendOsc`, or use the browser console to import/call the client send path during development.
5. Check gateway logs:

```sh
docker compose -f tools/kitu-web-admin/docker-compose.yml logs --no-color webtransport-gateway
```

6. Check the existing app server logs and Web Admin state updates.

## Fallback handling

The existing WebSocket endpoints are unchanged:

- `ws://localhost:8787/ws`
- `ws://localhost:8787/ws/runtime`

The Unity runtime client remains WebSocket-only for the MVP. The `/ws/runtime`
path is the authoritative runtime transport for MVP input, tick, and replay
validation because it is already stable, ordered, and easy to exercise in local
and CI environments.

WebTransport remains limited to the Web Admin / gateway experiment lane for now.
It is used to validate browser-originated KEP transport behavior without moving
runtime authority away from the established WebSocket path.

Future runtime transport backends should be selected by platform rather than
forcing one protocol everywhere:

- Unity Editor: WebSocket.
- Browser: WebTransport, with WebSocket fallback where appropriate.
- Native desktop on Windows, macOS, and Linux: WebSocket or QUIC.
- Native mobile: WebSocket or QUIC.

Native platforms should usually refer to a QUIC transport backend rather than a
WebTransport backend unless the implementation specifically adopts the
HTTP/3/WebTransport semantics. Authoritative runtime inputs must preserve the
ordering and delivery guarantees required by deterministic tick and replay
behavior regardless of transport.

If WebTransport is unavailable, fails TLS verification, or fails while sending, the browser falls back to WebSocket for OSC send attempts.

## Follow-up work

- Add a stable development TLS/certificate-hash workflow for browser verification.
- Keep a persistent internal WebSocket connection per WebTransport session instead of connecting once per stream.
- Stream multiple app-server KEP responses per WebTransport request if the protocol needs more than one response envelope.
- Add WebTransport datagram support for high-frequency real-time updates.
- Add integration tests that run the gateway against a demo-game container.
- Expand OSC packet support if bundles, blobs, or arrays become required.
