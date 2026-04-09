# Integration / Replay Framework Specification (P3 MVP)

This document defines the planning framework for integration runs and replay-oriented artifacts.
It establishes the directory layout and file contracts needed before implementing deterministic replay itself.

## Status

- Normative for repository layout and artifact shapes.
- Intentionally does not require a finished replay engine.
- Compatible with `doc/architecture.md`, where replay consumes event streams instead of patching ECS state directly.

## Goals

- Make integration and replay scenarios describable in repository data.
- Produce stable machine-readable artifacts for local runs and CI.
- Keep framework contracts independent from Unity-specific or transport-specific implementation details.

## Directory structure

Recommended layout for `kitu-integration-runner/`:

```text
kitu-integration-runner/
  README.md
  scenarios/
    README.md
    smoke/
      player-move-basic/
        scenario.json
        expected.json
        notes.md
  reports/
    README.md
  fixtures/
    README.md
  unity-app/
    .gitkeep
```

Directory intent:

- `scenarios/`: checked-in scenario definitions and expected results
- `reports/`: documented output/report format examples; generated run artifacts stay out of git
- `fixtures/`: reusable payload fragments or content fixtures if scenarios start sharing setup data
- `unity-app/`: Unity verification app used by CI/CD to validate that the app still boots and exchanges runtime boundary messages without regressions

Generated outputs should live outside the checked-in scenario tree, for example under a future `artifacts/` directory or tool-provided temp output directory.

## Scenario format

Each scenario directory represents one replayable contract test.

Required files:

- `scenario.json`
- `expected.json`

Optional files:

- `notes.md`
- extra fixture references

### `scenario.json`

Purpose:

- define initial context and ordered inbound messages for one scenario

Minimal shape:

```json
{
  "schema_version": 1,
  "scenario_id": "player-move-basic",
  "description": "single move intent produces one transform output on the next tick",
  "initial_state": {
    "content_version": "dev",
    "tick": 0
  },
  "steps": [
    {
      "at_tick": 0,
      "inbound": [
        {
          "channel": "unity",
          "address": "/input/move",
          "args": {
            "entity_id": "player:local",
            "x": 0.0,
            "y": 1.0
          }
        }
      ]
    }
  ]
}
```

Rules:

- `steps` are ordered and tick-indexed.
- inbound messages describe intents/envelopes, never direct state patches.
- scenario files may later grow setup fields, but the ordered input stream remains the core contract.

## Expected output format

`expected.json` captures the minimum assertions for a scenario.

Minimal shape:

```json
{
  "schema_version": 1,
  "scenario_id": "player-move-basic",
  "expected_outputs": [
    {
      "tick": 1,
      "address": "/render/player/transform",
      "args": {
        "entity_id": "player:local",
        "position": { "x": 0.0, "y": 1.0, "z": 0.0 }
      }
    }
  ],
  "expected_summary": {
    "status": "pass",
    "output_count": 1
  }
}
```

Rules:

- compare logical message content, not concrete wire bytes
- assert only the stable fields required by the scenario
- leave room for future partial-match or ignore-field semantics without changing the overall structure

## Run summary / report format

Each execution should produce one machine-readable summary file and may produce optional detailed reports.

### `summary.json`

Minimal shape:

```json
{
  "schema_version": 1,
  "run_id": "20260323T120000Z-player-move-basic",
  "scenario_id": "player-move-basic",
  "mode": "integration",
  "status": "pass",
  "started_at": "2026-03-23T12:00:00Z",
  "finished_at": "2026-03-23T12:00:01Z",
  "observed": {
    "output_count": 1,
    "mismatch_count": 0
  },
  "files": {
    "scenario": "kitu-integration-runner/scenarios/smoke/player-move-basic/scenario.json",
    "expected": "kitu-integration-runner/scenarios/smoke/player-move-basic/expected.json"
  }
}
```

Required meanings:

- `status`: overall result such as `pass`, `fail`, or `error`
- `observed.output_count`: number of logical outbound messages observed
- `observed.mismatch_count`: number of assertion mismatches

### Optional detailed reports

Examples:

- `events.ndjson`: ordered observed envelopes/messages
- `diff.json`: expected vs observed mismatch details
- `stdout.log`: tool log capture

These files are optional in the framework phase and must not be required for the framework to be considered defined.

## Determinism constraints for the framework

- Scenarios are defined as ordered inbound messages plus initial context.
- Expected outputs are defined in tick-relative, logical terms.
- Replay tooling must compare outputs generated through the same runtime boundary used by live input.
- No framework file may assume direct ECS-state patching as a replay mechanism.

## Relationship to implementation work

This framework is intentionally ready before:

- deterministic replay executor implementation
- authoritative queue internals
- Unity FFI expansion
- transport backend finalization

Those implementations should plug into this framework rather than redefining scenario or report contracts.
