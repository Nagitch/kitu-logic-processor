# Specifications

Protocol and behavior specifications for the reusable Kitu framework live here.

- `runtime-execution.md` and `runtime-execution-contract.md`: authoritative
  runtime tick order, authority rules and extension constraints.
- `kitu-envelope-protocol.md` and `transport-envelope.md`: transport-neutral
  envelope and ownership rules.
- `webtransport-gateway.md`: experimental local WebTransport gateway design,
  Docker shape and TLS notes.
- `osc-addressing.md`: address namespace rules and boundary ownership.
- `vertical-slice-player-move.md`: the generic movement slice contract.
- `integration-replay-framework.md`: runner layout, scenario files, expected
  outputs and summary/report files.
- `live-shell.md`: generic Shell command and result behavior.

The application-specific Arena specifications and frozen verification evidence
moved to the [Nagitch/kitu-unity-demo-game](https://github.com/Nagitch/kitu-unity-demo-game)
repository under `docs/specs/` and `docs/verification/`.
