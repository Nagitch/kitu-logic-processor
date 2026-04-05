# Project TODO

Structured roadmap and task status definitions for `kitu-logic-processor`.

## Status Definition

- `spec_only`
  - Design/specification exists, but implementation does not exist yet.
- `partial`
  - Implementation exists, but some required behavior, tests, or boundary validation are still missing.
- `implemented`
  - Implementation exists and all task-level tests pass.
- `verified`
  - Integration-level or replay-level validation is complete.

## Global Rules

### Task Selection Rules for Codex

- Always select tasks whose status is `spec_only` or `partial`.
- Do not modify tasks already marked `implemented` or `verified` unless the task is explicitly reopened.
- If behavior changes, update both:
  - `PROJECT_TODO.md`
  - `doc/architecture.md`
- A task is not complete until its `definition_of_done` is satisfied.
- Prefer one PR per task when practical.

### Task State Transition Rules

- `spec_only -> partial`
  - Initial implementation exists.
- `partial -> implemented`
  - All task-level tests pass.
  - No unresolved core TODO remains for that task.
- `implemented -> verified`
  - Integration, replay, or boundary-level validation passes.

## P0 — Fix Execution Semantics

status: implemented

goal:
Establish the authoritative runtime execution contract and baseline runtime loop behavior.

definition_of_done:
- runtime execution contract is documented
- baseline runtime loop follows documented phase ordering
- unit tests cover accumulator behavior and phase ordering
- architecture docs describe authoritative runtime boundary consistently

repository_state:
- `specs/runtime-execution-contract.md` defines tick execution order, input timing, transport polling timing, and output emission timing
- `doc/architecture.md` and `doc/detailed-flows.md` describe the authoritative runtime boundary
- `crates/kitu-runtime` implements the baseline loop contract with unit tests around accumulator behavior and phase ordering

remaining_work:
- Keep runtime docs and tests aligned whenever execution semantics change

### Task: Keep runtime docs and tests aligned with execution semantics

status: partial

context:
- P0 behavior exists, but future changes could desynchronize docs and tests.

input:
- runtime execution contract
- runtime loop implementation
- affected unit tests

output:
- synchronized docs and tests for current loop semantics

definition_of_done:
- changed runtime semantics are reflected in docs
- changed runtime semantics are reflected in tests
- no contradiction remains between specs and implementation comments

files_expected_to_change:
- `specs/runtime-execution-contract.md`
- `doc/architecture.md`
- `doc/detailed-flows.md`
- `crates/kitu-runtime/**`

## P1 — Raise `kitu-runtime` to a Minimum Viable Implementation

status: partial

goal:
Make `kitu-runtime` solid enough to support real gameplay slices, integration-level tests, and later replay execution.

definition_of_done:
- fixed-timestep `update(dt)` path is implemented and trusted as baseline
- authoritative input queue semantics are implemented and covered by tests
- output drain path is implemented and covered by tests
- `tick_once()` phase ordering is unit-tested
- first real end-to-end gameplay slice validates the runtime loop
- integration-level validation exists beyond crate-local unit tests

repository_state:
- fixed-timestep accumulator based `update(dt)` is implemented
- authoritative input queue exists with committed batch vs next-tick pending queue separation
- output buffer / drain API is implemented
- `tick_once()` phase ordering is implemented and unit-tested
- ordering semantics are covered by `kitu-runtime` unit tests

remaining_work:
- Re-evaluate whether P1 can be considered complete after the vertical slice and replay smoke path exist

### Task: Validate the runtime loop through the first real gameplay slice

status: implemented

context:
- runtime core needs validation through a concrete gameplay flow, not only isolated unit tests

input:
- current runtime loop
- first vertical slice contract

output:
- evidence that runtime semantics hold under a real gameplay slice

definition_of_done:
- a real slice executes through the runtime loop
- the observed behavior matches the documented execution contract
- failing conditions are documented if found

files_expected_to_change:
- `specs/vertical-slice-player-move.md`
- `crates/kitu-runtime/**`
- related tests

### Task: Expand testing beyond `kitu-runtime` unit tests into integration-level validation

status: implemented

context:
- unit tests alone are insufficient to prove slice-level correctness

input:
- runtime public behavior
- integration test structure

output:
- integration-level validation path

definition_of_done:
- at least one integration-level validation path exists
- integration-level checks exercise public runtime behavior
- test result can fail when loop ordering or queue semantics regress

files_expected_to_change:
- integration test locations under workspace
- `kitu-integration-runner/**`
- CI-related files if needed

### Task: Re-evaluate P1 completion after vertical slice and replay smoke path exist

status: spec_only

context:
- P1 should be closed only when runtime viability is validated through real slice and replay foundations

input:
- P1 repository state
- P2 slice state
- P3 smoke path state

output:
- explicit completion or continued partial status for P1

definition_of_done:
- P1 status is explicitly justified in `PROJECT_TODO.md`
- `doc/architecture.md` staging text matches the decision
- no contradiction remains between roadmap and architecture staging

files_expected_to_change:
- `PROJECT_TODO.md`
- `doc/architecture.md`

## P2 — Build the Minimum Vertical Slice

status: partial

goal:
Deliver the minimum end-to-end gameplay slice for player movement from input to authoritative state update to render event.

definition_of_done:
- `/input/move` is accepted and validated
- deterministic authoritative state update is applied
- `/render/player/transform` is emitted
- minimum runtime-to-host boundary is defined and implemented
- `kitu-unity-ffi` is extended only as much as needed for the slice
- slice behavior is covered by unit and integration tests

repository_state:
- `specs/vertical-slice-player-move.md` defines `/input/move` -> authoritative state update -> `/render/player/transform`
- `kitu-runtime` processes `/input/move` into authoritative position updates and emits `/render/player/transform`

remaining_work:
- Define and implement the minimum boundary between runtime and Unity / host
- Extend `kitu-unity-ffi` only as much as needed for the slice

### Task: Implement the minimum input payload handling for `/input/move`

status: implemented

context:
- the vertical slice depends on stable conversion from event payload to internal input representation

input:
- OSC message `/input/move`

output:
- validated internal input structure for the movement slice

definition_of_done:
- invalid payload is rejected
- valid payload is converted into internal input format
- unit tests exist for both valid and invalid cases

files_expected_to_change:
- `crates/kitu-runtime/**`
- related movement/input tests
- `specs/vertical-slice-player-move.md` if contract text changes

### Task: Implement deterministic authoritative state update for the player move slice

status: implemented

context:
- the runtime owns state and must apply the move slice deterministically

input:
- validated move input
- current authoritative state

output:
- updated authoritative player position/state

definition_of_done:
- same input sequence produces identical results across runs
- update occurs in tick-based runtime flow
- unit tests verify deterministic behavior

files_expected_to_change:
- `crates/kitu-runtime/**`
- slice-level tests

### Task: Emit `/render/player/transform` from the runtime-owned output path

status: implemented

context:
- presentation layer must receive runtime-owned transform output, not derive gameplay state itself

input:
- authoritative player state after simulation

output:
- runtime-owned `/render/player/transform` event

definition_of_done:
- event is emitted through the runtime output path
- payload matches slice contract
- tests verify expected emission

files_expected_to_change:
- `crates/kitu-runtime/**`
- output-related tests
- `specs/vertical-slice-player-move.md` if payload contract changes

### Task: Define and implement the minimum boundary between runtime and Unity / host

status: spec_only

context:
- runtime behavior exists, but boundary ownership and minimal host contract remain incomplete

input:
- current vertical slice behavior
- current FFI/host integration shape

output:
- minimal runtime-to-host contract for the slice

definition_of_done:
- boundary ownership is documented
- host/runtime responsibilities are explicit
- the slice can be executed through the documented boundary
- at least one boundary-level test or smoke check exists

files_expected_to_change:
- `doc/architecture.md`
- relevant `specs/*.md`
- `crates/kitu-unity-ffi/**`
- host/boundary test code if present

### Task: Extend `kitu-unity-ffi` only as much as needed for the slice

status: spec_only

context:
- FFI should remain minimal and presentation-oriented

input:
- minimum vertical slice boundary

output:
- smallest FFI surface needed for movement slice execution

definition_of_done:
- only slice-required FFI APIs are added or updated
- no gameplay rules move into Unity boundary
- tests or smoke checks cover the introduced FFI path

files_expected_to_change:
- `crates/kitu-unity-ffi/**`
- `doc/architecture.md`
- boundary docs/specs as needed

## P3 — Prepare Integration / Replay Foundations

status: partial

goal:
Create the minimum checked-in scenario/replay foundation needed to validate authoritative runtime behavior through reproducible inputs and expected outputs.

definition_of_done:
- smoke scenario exists in `kitu-integration-runner`
- first checked-in `scenario.json` and `expected.json` exist
- minimal replay runner or CLI entry point produces `summary.json`
- replay inputs and assertions are aligned with authoritative runtime boundaries

repository_state:
- `specs/integration-replay-framework.md` defines scenario, expected output, and summary/report contracts
- `kitu-integration-runner/` directory structure is present
- checked-in smoke scenarios are not added yet
- `tools/kitu-replay-runner` is only a placeholder entry point today

remaining_work:
- Add a smoke scenario to `kitu-integration-runner`
- Add the first checked-in `scenario.json` and `expected.json`
- Implement a minimal replay runner or CLI entry point that produces `summary.json`
- Keep replay inputs and assertions aligned with the authoritative runtime boundary

### Task: Add a smoke scenario to `kitu-integration-runner`

status: spec_only

context:
- runner structure exists, but no checked-in executable baseline scenario exists yet

input:
- integration replay framework contract
- current vertical slice behavior

output:
- minimal smoke scenario checked into repository

definition_of_done:
- repository contains at least one runnable smoke scenario
- scenario uses current authoritative runtime boundary
- scenario is referenced by test or runner documentation

files_expected_to_change:
- `kitu-integration-runner/**`
- related docs/specs

### Task: Add the first checked-in `scenario.json` and `expected.json`

status: spec_only

context:
- replay foundation needs stable input and expected-output fixtures

input:
- smoke scenario format
- current authoritative output expectations

output:
- initial checked-in fixture pair for scenario and expected output

definition_of_done:
- `scenario.json` exists
- `expected.json` exists
- fixture content matches current vertical slice and runtime boundary

files_expected_to_change:
- `kitu-integration-runner/**`
- `specs/integration-replay-framework.md` if format clarification is needed

### Task: Implement a minimal replay runner or CLI entry point that produces `summary.json`

status: spec_only

context:
- placeholder exists, but replay validation cannot run end-to-end yet

input:
- checked-in scenario fixture
- expected output fixture

output:
- runnable replay path that produces summary output

definition_of_done:
- replay runner accepts scenario input
- replay runner compares or reports against expected output path
- `summary.json` is produced in the documented format

files_expected_to_change:
- `tools/kitu-replay-runner/**`
- `kitu-integration-runner/**`
- CLI docs if needed

### Task: Keep replay inputs and assertions aligned with the authoritative runtime boundary

status: spec_only

context:
- replay fixtures can easily drift from actual runtime ownership and event boundaries

input:
- replay framework contract
- architecture/runtime boundary docs
- current runtime behavior

output:
- replay validation that reflects actual authoritative runtime rules

definition_of_done:
- replay inputs do not bypass authoritative runtime boundary
- assertions are based on documented runtime-owned outputs
- docs/specs do not contradict runner behavior

files_expected_to_change:
- `specs/integration-replay-framework.md`
- `doc/architecture.md`
- `kitu-integration-runner/**`
- `tools/kitu-replay-runner/**`

## P4 — Organize Surrounding Specs and Maintenance Docs

status: partial

goal:
Keep specs, architecture, crate docs, and roadmap aligned so implementation can proceed without ambiguity.

definition_of_done:
- core specs under `specs/` reflect the actual repository state
- crate READMEs and architecture docs match current implementation staging
- roadmap status text is synchronized with architecture staging
- future TSQ1 / TMD / scripting entry points are documented without implying premature implementation commitments

repository_state:
- core specs exist under `specs/`
- CI, crate READMEs, and core architecture docs are in place
- `PROJECT_TODO.md` is aligned to the current P0-P4 roadmap structure at a high level

remaining_work:
- Keep `PROJECT_TODO.md`, crate READMEs, and spec status text in sync
- Clarify done vs pending work whenever design-only milestones transition into implementation
- Prepare entry points for future TSQ1 / TMD / scripting expansion without overcommitting implementation order

### Task: Keep `PROJECT_TODO.md`, crate READMEs, and spec status text in sync

status: partial

context:
- current repository already has docs and specs, but state text can drift over time

input:
- roadmap text
- crate README state descriptions
- current specs and architecture text

output:
- synchronized status wording across project-facing docs

definition_of_done:
- milestone wording does not contradict architecture staging
- crate READMEs do not overstate implementation status
- current repository state is described consistently across key docs

files_expected_to_change:
- `PROJECT_TODO.md`
- `doc/architecture.md`
- crate `README.md` files
- affected `specs/*.md`

### Task: Clarify done vs pending work whenever design-only milestones transition into implementation

status: partial

context:
- partial implementation often leaves ambiguous wording about what is actually complete

input:
- milestone states
- recently implemented work

output:
- explicit distinction between design-only, partial, implemented, and verified work

definition_of_done:
- milestone/task wording uses shared status vocabulary
- design-only work is not described as implementation-complete
- pending implementation is visible in roadmap text

files_expected_to_change:
- `PROJECT_TODO.md`
- `doc/architecture.md`
- affected specs if wording needs correction

### Task: Prepare entry points for future TSQ1 / TMD / scripting expansion without overcommitting implementation order

status: spec_only

context:
- future subsystems should be represented in docs, but not falsely implied as near-term implemented work

input:
- current architecture direction
- current repository maturity

output:
- documented future entry points with explicit non-commitment on exact implementation order

definition_of_done:
- architecture docs describe likely extension points
- roadmap text does not imply implementation that does not exist
- future work is framed as staged or deferred where appropriate

files_expected_to_change:
- `doc/architecture.md`
- `PRODUCTION_STRATEGY.md`
- related specs/docs as needed

## Cross-Cutting Backlog

### Task: Expand unit test coverage across existing crates/modules

status: partial

definition_of_done:
- newly added public APIs have tests
- key runtime and boundary modules gain coverage where missing
- regressions in core execution flow become easier to catch

### Task: Add integration tests covering sample logic flows

status: partial

definition_of_done:
- at least one sample logic flow is covered outside crate-local unit tests
- integration tests fail on runtime boundary regressions

### Task: Add deterministic replay test coverage for tick/event streams

status: spec_only

definition_of_done:
- replayable tick/event stream validation exists
- repeated runs over the same scenario produce stable results

### Task: Publish-readiness checklist per crate

status: partial

definition_of_done:
- each candidate crate has publish-readiness checks documented
- missing metadata or packaging issues are tracked explicitly

### Task: Fill in package metadata and `include` directives

status: partial

definition_of_done:
- crates intended for future publication have required metadata
- packaging scope is explicitly controlled where needed

### Task: Validate `cargo publish --dry-run` where applicable

status: spec_only

definition_of_done:
- dry-run validation is executed for relevant crates
- discovered packaging issues are tracked or fixed
