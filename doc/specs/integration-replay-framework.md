# Integration / Replay Framework Specification (P3 MVP)

This document defines the planning framework for integration runs and replay-oriented artifacts.
It establishes the directory layout and file contracts used by the minimal replay runner and future replay expansion.

## Status

- Normative for repository layout and artifact shapes.
- Intentionally does not require a full replay engine beyond the current smoke runner.
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
          "channel": "runtime",
          "address": "/input/move",
          "args": {
            "entity_id": "player:local",
            "x": 1.5,
            "y": 2.0
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
- `channel` identifies the boundary origin class; smoke replay uses `runtime` to mean direct runtime-boundary input.
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
        "source_tick": 0,
        "position": { "x": 1.5, "y": 2.0, "z": 0.0 }
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
- `expected_outputs[].tick` is the externally visible output tick; `args.source_tick` is the authoritative tick that produced the transform.
- leave room for future partial-match or ignore-field semantics without changing the overall structure

## Run summary / report format

Each execution should produce one machine-readable summary file and may produce optional detailed reports.

### `summary.json`

Minimal shape:

```json
{
  "schema_version": 1,
  "run_id": "player-move-basic-pass",
  "scenario_id": "player-move-basic",
  "mode": "integration",
  "status": "pass",
  "started_at": "1970-01-01T00:00:00Z",
  "finished_at": "1970-01-01T00:00:00Z",
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
- `started_at` and `finished_at` may be deterministic sentinel timestamps for smoke replay summaries where wall-clock time is intentionally excluded.

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

This framework is intentionally transport- and host-neutral. Current movement-slice runtime and Unity FFI work should plug into these scenario and report contracts rather than redefining them.

Implemented smoke path:

- checked-in movement-slice smoke scenario and expected output fixtures
- minimal replay runner that reads the fixture pair and writes `summary.json`

Still-pending implementation work:

- broader deterministic replay executor capabilities beyond the first smoke path
- richer mismatch reports such as `diff.json`
- transport backend finalization
