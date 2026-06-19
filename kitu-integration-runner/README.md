# Kitu Integration Runner

This directory holds integration and replay scenarios plus the framework definition for replay execution.
At the current stage, the repository includes the first smoke fixture pair and a minimal replay runner for that contract.

## Current status

- Framework contract is defined.
- `scenarios/smoke/player-move-basic/` contains the first runtime-boundary smoke fixture.
- `tools/kitu-replay-runner` can run the smoke fixture pair and produce `summary.json`.
- Broader replay coverage, richer reports, and Unity verification remain staged work.

## Normative spec

Use [`doc/specs/integration-replay-framework.md`](../doc/specs/integration-replay-framework.md) as the current source of truth for:

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
  unity-demo-game/
```

Generated run artifacts should be emitted outside the checked-in scenario files, for example through the replay runner's `--output-dir`.

## Unity demo-game verification app (`unity-demo-game/`)

- `unity-demo-game/` is reserved for the minimal Unity client project used in CI/CD and integration testing.
- It mirrors `apps/demo-game` at the Unity presentation/input boundary so Rust-side demo-game checks and Unity smoke checks can be named consistently.
- Its purpose is regression detection (boot/runtime-boundary smoke checks), not full game implementation hosting.
- Keep the project lean and test-focused so it remains stable as an infrastructure asset.

## Design constraints

- Replay must consume ordered runtime-boundary messages, not direct ECS state patches.
- Unity remains a presentation/input boundary and is not required for the initial scenario contract.
- Transport remains delivery-only and must not become a gameplay logic layer.
- The framework must stay compatible with the current runtime contract and future vertical-slice implementation work.
