# Project TODO

Project-wide TODO list for the *kitu-logic-processor* repository.
This file is maintained by both human contributors and AI assistants.


## Guidelines

- Keep this file **simple and scannable**.
- Use short bullet points.
- When a task is completed, remove it or move it to a "Done" section if desired.
- AI assistants should update this file whenever implementation or design work progresses.


## TODO

- [x] Additional development environment setup
  - [x] Add helper scripts/justfile entries for common workflows
  - [x] Verify rust-toolchain and devcontainer match CI expectations
- [x] Establish continuous integration
  - [x] fmt/clippy/test workflow in GitHub Actions
  - [x] Basic status badges in README
- [x] Define core runtime architecture
  - [x] Document module boundaries (timeline, ecs, bridge)
  - [x] Capture invariants and determinism requirements in `doc/architecture.md`
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
