# Specifications

Protocol and behavior specifications live here.

- `runtime-execution.md`: runtime execution boundary, authority rules, and extension constraints for the MVP.
- `runtime-execution-contract.md`: authoritative runtime tick order, input timing, transport polling timing, and output emission timing.
- `kitu-envelope-protocol.md`: KEP MessagePack envelope shape for transport-independent payload metadata.
- `transport-envelope.md`: transport-neutral logical envelope fields and ownership rules.
- `webtransport-gateway.md`: experimental local WebTransport gateway design, Docker Compose shape, TLS notes, and fallback behavior.
- `osc-addressing.md`: address namespace rules and boundary ownership for OSC-like messages.
- `vertical-slice-player-move.md`: P2 minimum slice contract for `/input/move` -> authoritative state update -> `/render/player/transform`.
- `integration-replay-framework.md`: P3 framework for runner layout, scenario files, expected outputs, and summary/report files.
