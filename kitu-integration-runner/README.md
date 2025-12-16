# Kitu Integration Runner Contract

This document defines the **integration (combined) test execution contract** for `kitu-integration-runner`.
The goal is to ensure that the same procedures and artifacts can be obtained both in CI (GitHub Actions)
and in local execution.

---

## 1. Directory Structure

```
kitu-integration-runner/
  backend/                 # Rust backend (server-mode execution)
  client/                  # Unity project (test application)
  scenarios/               # Scenario assets (TSQ1 / DSL / expectations)
  scripts/                 # Shared scripts for local and CI execution
  artifacts/               # Execution outputs (logs / TSQ1 / DB dumps) [gitignored]
  kitu-integration-runner-README.md
```

### 1.1 `scenarios/` Standard Layout

```
scenarios/
  UC-01-game-startup/
    input.tsq1
    expect.json
    note.md
  UC-20-battle-core/
    input.tsq1
    expect.json
```

* One use case equals one directory
* `input.tsq1` is generally required
* `expect.json` defines minimal machine-readable assertions (optional but recommended)

---

## 2. Test Modes (Execution Matrix)

Integration tests support the following modes.

### 2.1 Headless Mode (Server Mode)

* `backend` runs as an external process
* `client` interacts via external interfaces (Web API / OSC, etc.)
* CI execution uses headless Unity (`-batchmode -nographics`)

### 2.2 Library Mode (Embedded Mode)

* `client` embeds Rust application logic as a library (FFI / in-process)
* The same scenarios must be executable both locally and in CI

---

## 3. Client CLI Contract (Mandatory)

The `client` Unity build artifact **must** conform to the following CLI contract.

### 3.1 Common Arguments

| Argument                                        |    Required | Description                                           |
| ----------------------------------------------- | ----------: | ----------------------------------------------------- |
| `--mode <headless\|library\|replay>`            |         yes | Execution mode                                        |
| `--scenario <SCENARIO_ID>`                      |   test: yes | Target directory under `scenarios/`                   |
| `--tsq1 <PATH>`                                 | replay: yes | TSQ1 file for replay                                  |
| `--artifacts <DIR>`                             |          no | Output directory (default: `artifacts/`)              |
| `--seed <INT>`                                  |          no | RNG seed (default: fixed value, recommended 0)        |
| `--timeout-sec <INT>`                           |          no | Timeout in seconds (default: scenario-defined or 60s) |
| `--log-level <trace\|debug\|info\|warn\|error>` |          no | Log verbosity                                         |

### 3.2 Unity Startup Flags (CI)

CI execution should pass the following Unity Player flags:

* `-batchmode`
* `-nographics`
* `-logFile <PATH>`

The build **should also run correctly without `-nographics`** to allow visual replay locally.

### 3.3 Exit Codes (Mandatory)

| Code | Meaning                             |
| ---: | ----------------------------------- |
|  `0` | Success                             |
| `10` | Assertion failure                   |
| `20` | Timeout                             |
| `30` | Invalid scenario / input            |
| `40` | Backend unavailable (headless mode) |
| `50` | Unexpected crash or exception       |

---

## 4. Artifacts Contract (Mandatory)

Test execution must emit artifacts regardless of success or failure.

### 4.1 Output Location

* Default: `kitu-integration-runner/artifacts/`
* Overridable via `--artifacts <DIR>`

### 4.2 Standard Layout

```
artifacts/
  <RUN_ID>/
    summary.json
    client/
      player.log
      output.tsq1
      state.json
      db/
    backend/
      server.log
      db/
```

#### RUN_ID Naming Convention (Recommended)

`YYYYMMDD-HHMMSS_<mode>_<scenario>`

Example:

```
20251212-134455_headless_UC-20-battle-core
```

### 4.3 `summary.json` (Mandatory)

```json
{
  "run_id": "20251212-134455_headless_UC-20-battle-core",
  "mode": "headless",
  "scenario": "UC-20-battle-core",
  "exit_code": 10,
  "duration_ms": 12345,
  "seed": 0,
  "artifacts_dir": "artifacts/20251212-134455_headless_UC-20-battle-core",
  "inputs": {
    "tsq1": "scenarios/UC-20-battle-core/input.tsq1"
  },
  "outputs": {
    "output_tsq1": "artifacts/.../client/output.tsq1",
    "player_log": "artifacts/.../client/player.log"
  }
}
```

### 4.4 TSQ1 Handling (Mandatory)

* `scenarios/<SCENARIO_ID>/input.tsq1` is the canonical input
* `client/output.tsq1` must be emitted after execution
* Even on failure, partial TSQ1 output should be preserved if possible

---

## 5. Backend Contract (Headless Mode)

### 5.1 Startup / Shutdown

* Backend startup is centralized in `scripts/run_backend.sh`
* Backend must terminate cleanly on SIGTERM

### 5.2 Ports

* Default ports are fixed (e.g. HTTP 18080, OSC 9000)
* Ports should be overridable via environment variables (recommended)

---

## 6. Scripts Contract (Strongly Recommended)

To minimize CI/local divergence, the following scripts should exist:

* `scripts/build_client_linux.sh`
* `scripts/run_client.sh`
* `scripts/run_backend.sh`
* `scripts/run_suite.sh`

### 6.1 `run_suite.sh` Interface

Examples:

```
./scripts/run_suite.sh --mode headless --scenario UC-20-battle-core
./scripts/run_suite.sh --mode library --scenario UC-20-battle-core
./scripts/run_suite.sh --mode headless --group smoke
```

Scenario groups may be defined under `scenarios/_groups/`.

---

## 7. GitHub Actions Policy

CI execution policy:

* Build Unity client for Linux
* Execute both headless and library modes via matrix
* Always upload `artifacts/` on failure

### 7.1 Recommended Priority

* Pull Requests: smoke group only
* Main merge / nightly: full suite

---

## 8. Replay Mode

### 8.1 Purpose

Replay mode allows reproducing CI failures locally using saved TSQ1 files with rendering enabled.

### 8.2 CLI Usage

```
--mode replay --tsq1 <path> [--seed] [--timeout-sec]
```

### 8.3 Expected Behavior

* Apply TSQ1 events to the simulation timeline
* Regenerate `output.tsq1`
* Allow visual inspection and optional diffing

---

## 9. Minimum Adoption Checklist

* [ ] Client supports `--mode headless/library/replay`
* [ ] Exit codes follow the defined contract
* [ ] `summary.json`, `output.tsq1`, and `player.log` are always emitted
* [ ] Headless mode communicates with backend correctly
* [ ] Library mode runs Rust logic in-process
* [ ] CI uploads artifacts on failure
