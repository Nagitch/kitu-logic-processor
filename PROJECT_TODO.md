# Project TODO

Project-wide TODO list for the *kitu-logic-processor* repository.
This file is maintained by both human contributors and AI assistants.


## Guidelines

- Keep this file **simple and scannable**.
- Use short bullet points.
- When a task is completed, remove it or move it to a "Done" section if desired.
- AI assistants should update this file whenever implementation or design work progresses.


## TODO

- [ ] P1 runtime core implementation remains partial / frozen
  - [ ] Do not mark P1 complete
  - [ ] `kitu-runtime` full `update(dt)` implementation is deferred
  - [ ] authoritative input queue implementation is deferred
  - [ ] output drain implementation is deferred
  - [ ] deterministic replay execution implementation is deferred
- [x] Additional development environment setup
  - [x] Add helper scripts/justfile entries for common workflows
  - [x] Verify rust-toolchain and devcontainer match CI expectations
- [x] Establish continuous integration
  - [x] fmt/clippy/test workflow in GitHub Actions
  - [x] Basic status badges in README
- [x] Define core runtime architecture
  - [x] Document module boundaries (timeline, ecs, bridge)
  - [x] Capture invariants and determinism requirements in `doc/architecture.md`
- [x] P2 minimum vertical slice specification
  - [x] Define `/input/move` contract
  - [x] Define authoritative state update boundary
  - [x] Define `/render/player/transform` contract
- [x] P3 integration / replay framework design
  - [x] Define `kitu-integration-runner` directory structure
  - [x] Define scenario format
  - [x] Define expected output format
  - [x] Define summary/report format
- [x] P4 surrounding specifications and maintenance docs
  - [x] Add initial specs under `specs/`
  - [x] Update `PROJECT_TODO.md` to match the current state
  - [x] Record that P1 is partial / frozen
- [ ] Testing baseline
  - [ ] Expand unit test coverage across existing crates/modules
  - [ ] Add integration tests covering sample logic flows
  - [ ] Add deterministic replay test coverage for tick/event streams
- [ ] Publish-readiness checklist per crate
  - [ ] Fill in package metadata and `include` directives
  - [ ] Validate `cargo publish --dry-run` where applicable


## Done

- [x] .devcontainer setup
  - [x] review and merge
- [x] Create initial project directory structure
  - [x] review and merge
- [x] [Codex] something done
  - [x] review and merge
