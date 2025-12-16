# Project TODO

Project-wide TODO list for the *kitu-logic-processor* repository.
This file is maintained by both human contributors and AI assistants.


## Guidelines

- Keep this file **simple and scannable**.
- Use short bullet points.
- When a task is completed, remove it or move it to a "Done" section if desired.
- AI assistants should update this file whenever implementation or design work progresses.


## TODO

- [ ] Additional development environment setup
  - [ ] Add helper scripts/justfile entries for common workflows
  - [ ] Verify rust-toolchain and devcontainer match CI expectations
- [ ] Establish continuous integration
  - [ ] fmt/clippy/test workflow in GitHub Actions
  - [ ] Basic status badges in README
- [ ] Define core runtime architecture
  - [ ] Document module boundaries (timeline, ecs, bridge)
  - [ ] Capture invariants and determinism requirements in `doc/architecture.md`
- [ ] Testing baseline
  - [ ] Add unit tests for existing crates/modules
  - [ ] Add integration tests covering sample logic flows
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
