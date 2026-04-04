# Project TODO

Project-wide TODO list for the *kitu-logic-processor* repository.
This file is maintained by both human contributors and AI assistants.


## Guidelines

- Keep this file **simple and scannable**.
- Use short bullet points.
- When a task is completed, remove it or move it to a "Done" section if desired.
- AI assistants should update this file whenever implementation or design work progresses.


## Roadmap

### P0 — Fix execution semantics

Status:

- [x] Runtime execution contract is defined and documented.

Repository state:

- [x] `specs/runtime-execution-contract.md` defines tick execution order, input timing, transport polling timing, and output emission timing.
- [x] `doc/architecture.md` and `doc/detailed-flows.md` describe the authoritative runtime boundary.
- [x] `crates/kitu-runtime` implements the baseline loop contract with unit tests around accumulator behavior and phase ordering.

Remaining work:

- [ ] Keep runtime docs and tests aligned whenever execution semantics change.

### P1 — Raise `kitu-runtime` to a minimum viable implementation

Status:

- In progress: first end-to-end gameplay slice validation and integration-level tests now exist; final completion review is pending.

Repository state:

- [x] Fixed-timestep accumulator based `update(dt)` is implemented.
- [x] Authoritative input queue exists with committed batch vs next-tick pending queue separation.
- [x] Output buffer / drain API is implemented.
- [x] `tick_once()` phase ordering is implemented and unit-tested.
- [x] Ordering semantics are covered by `kitu-runtime` unit tests.

Remaining work:

- [x] Validate the current runtime loop through the first real end-to-end gameplay slice.
- [x] Expand testing beyond `kitu-runtime` unit tests into integration-level validation.
- [ ] Re-evaluate whether P1 can be considered complete after the vertical slice and replay smoke path exist.

### P2 — Build the minimum vertical slice

Status:

- In progress: the player move slice is now implemented in runtime, while Unity/host boundary work is still pending.

Repository state:

- [x] `specs/vertical-slice-player-move.md` defines `/input/move` -> authoritative state update -> `/render/player/transform`.
- [x] `kitu-runtime` processes `/input/move` into authoritative position updates and emits `/render/player/transform`.

Remaining work:

- [x] Implement the minimum input payload handling for `/input/move`.
- [x] Implement deterministic authoritative state update for the player move slice.
- [x] Emit `/render/player/transform` from the runtime-owned output path.
- [ ] Define and implement the minimum boundary between runtime and Unity / host.
- [ ] Extend `kitu-unity-ffi` only as much as needed for the slice.

### P3 — Prepare integration / replay foundations

Status:

- In progress: framework contract is defined; runner implementation is still pending.

Repository state:

- [x] `specs/integration-replay-framework.md` defines scenario, expected output, and summary/report contracts.
- [x] `kitu-integration-runner/` directory structure is present.
- [ ] Checked-in smoke scenarios are not added yet.
- [ ] `tools/kitu-replay-runner` is only a placeholder entry point today.

Remaining work:

- [ ] Add a smoke scenario to `kitu-integration-runner`.
- [ ] Add the first checked-in `scenario.json` and `expected.json`.
- [ ] Implement a minimal replay runner or CLI entry point that produces `summary.json`.
- [ ] Keep replay inputs and assertions aligned with the authoritative runtime boundary.

### P4 — Organize surrounding specs and maintenance docs

Status:

- In progress: initial documentation set exists; maintenance is ongoing.

Repository state:

- [x] Core specs exist under `specs/`.
- [x] CI, crate READMEs, and core architecture docs are in place.
- [x] `PROJECT_TODO.md` is aligned to the current P0-P4 roadmap structure.

Remaining work:

- [ ] Keep `PROJECT_TODO.md`, crate READMEs, and spec status text in sync.
- [ ] Clarify done vs pending work whenever design-only milestones transition into implementation.
- [ ] Prepare entry points for future TSQ1 / TMD / scripting expansion without overcommitting implementation order.

## Cross-cutting backlog

- [ ] Testing baseline
  - [ ] Expand unit test coverage across existing crates/modules
  - [ ] Add integration tests covering sample logic flows
  - [ ] Add deterministic replay test coverage for tick/event streams
- [ ] Publish-readiness checklist per crate
  - [ ] Fill in package metadata and `include` directives
  - [ ] Validate `cargo publish --dry-run` where applicable

## Current practical interpretation

- P0 is defined and has a baseline implementation.
- P1 should be treated as active baseline work, not as frozen/deferred work.
- P2 and P3 are the next concrete implementation milestones.
- P4 is ongoing maintenance work that should track the real state of the repository.


## Done

- [x] .devcontainer setup
  - [x] review and merge
- [x] Create initial project directory structure
  - [x] review and merge
- [x] [Codex] something done
  - [x] review and merge
