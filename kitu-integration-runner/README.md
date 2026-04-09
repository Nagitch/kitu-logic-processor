# Kitu Integration Runner

This directory holds the framework definition for future integration and replay execution.
At the current stage, the repository defines the checked-in scenario and report contracts first, while deterministic replay implementation remains deferred.

## Current status

- Framework/design phase only.
- Deterministic replay executor is not implemented yet.
- The authoritative runtime loop baseline exists in `kitu-runtime`, but replay integration on top of it is not implemented yet.

## Normative spec

Use [`specs/integration-replay-framework.md`](../specs/integration-replay-framework.md) as the current source of truth for:

- directory structure
- scenario format
- expected output format
- summary/report format

## Intended repository layout

```text
kitu-integration-runner/
  README.md
  scenarios/
  reports/
  fixtures/
  unity-app/
```

Generated run artifacts should be emitted outside the checked-in scenario files, for example under a future `artifacts/` directory.

## Unity verification app (`unity-app/`)

- `unity-app/` is reserved for a minimal Unity project used in CI/CD and integration testing.
- Its purpose is regression detection (boot/runtime-boundary smoke checks), not full game implementation hosting.
- Keep the project lean and test-focused so it remains stable as an infrastructure asset.

## Design constraints

- Replay must consume ordered runtime-boundary messages, not direct ECS state patches.
- Unity remains a presentation/input boundary and is not required for the initial scenario contract.
- Transport remains delivery-only and must not become a gameplay logic layer.
- The framework must stay compatible with the current runtime contract and future vertical-slice implementation work.
