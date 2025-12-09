# AGENT.md — Kitu Logic Processor

This document explains how AI assistants (e.g. ChatGPT / Codex) should work on this repository.

---

## 1. Project overview

### 1.1 What this repository is

- Name: **kitu-logic-processor**

- Role in the ecosystem:

  - Core game / app logic runtime for the broader **Kitu** ecosystem.

  - Provides deterministic, testable logic execution independent of any specific frontend (Unity, etc.).

  - Bridges data-driven formats (TSQ1, Tanu Markdown / DB-backed configs, etc.) and ECS-style runtime.

The repository will eventually provide:

- A robust **Rust library** exposing the core logic runtime.

- One or more **CLI tools** for local testing, playback, and integration tests.

- A clean **API surface** that frontends (Unity, tools) can call into.

### 1.2 Current maturity level

- Early stage, design-heavy.

- Acceptable for AI assistants to:

  - Propose alternative structures.

  - Create missing modules.

  - Iterate on dev tooling and CI.

---

## 2. Intended repository structure

AI assistants may create missing parts if intent is clear.

- `src/`

  - `lib.rs`

  - `bin/`

  - `ecs/`

  - `timeline/`

  - `bridge/`

- `examples/`

- `doc/`

  - `architecture.md`

  - `dev-workflow.md`

- `.devcontainer/`

  - `devcontainer.json`

  - (optional) `Dockerfile`

- `.github/workflows/`

- `AGENT.md`

- `CONTRIBUTING.md`

- `README.md`, `README_JP.md (optional, separate JP version not required for this document)`

---

## 3. Goals and non-goals

### Goals

- Deterministic, replayable logic runtime.

- Data-driven workflows (TSQ1, TMD).

- Testable and tool-friendly.

- Frontend-agnostic.

### Non-goals

- Frontend-heavy logic (Unity, Unreal) inside core.

---

## 4. How AI assistants should work

### 4.1 Allowed and encouraged

AI assistants must also reference and maintain **PRODUCTION_STRATEGY.md** when making changes to project structure, development environment, or repository-wide practices. This file serves as the high-level blueprint for setting up and evolving the project's foundational infrastructure.

1. **Code implementation** (Rust modules / traits / tests)

2. **Testing** (unit, integration, scenario)

3. **Documentation**

   - Maintain all documents under the `doc/` directory.

   - Always treat the **English version as the primary source of truth**.

   - Whenever creating or updating any document, **also update the corresponding Japanese version** (e.g., `*_JP.md`) to keep both synchronized.

   - Add new documents if needed and ensure both EN/JP versions remain consistent.

4. **Development environment maintenance****

   - Update devcontainer

   - Update Dockerfile

   - Maintain toolchain settings

   - Maintain Clippy, rustfmt, justfile

   - Maintain CI workflows (fmt/clippy/test)

5. **Housekeeping** (gitignore, editorconfig)** (gitignore, editorconfig)

### 4.2 Needs caution

- Introducing crates with unclear licenses.

- Large-scale refactors.

### 4.3 Not desired

- Copying external code without license validation.

- Shifting high-level architecture without instruction.

---

## 5. Coding guidelines

* Edition 2021 or later.
* rustfmt = source of truth.
* Clippy should warn-as-error.
* Prefer `thiserror` in libraries.
* Use `anyhow` for CLI top levels.
* Use `tracing` ecosystem.
* For any crate or library in this repository, **every public API (public functions, types, traits, and methods) MUST have at least one unit test or integration test** exercising its intended behavior. If an exception is necessary, it should be clearly documented in code comments or relevant docs.

---

## 6. Development workflow

1. Use devcontainer.

2. Before commit:

   - `cargo fmt --all`

   - `cargo clippy --all-targets --all-features -- -D warnings`

   - `cargo test --all`

3. CI should match local setup.

---

## 7. How AI should apply changes

1. Describe intent first.

2. Small steps.

3. Keep this file updated when workflow/tooling changes.

---

## 8. Open tasks / TODO for AI

All project-level tasks should be managed in a **separate file** named `PROJECT_TODO.md` at the repository root.

AI assistants should:

- Add new tasks to `PROJECT_TODO.md` instead of listing them here.

- Update or close tasks in `PROJECT_TODO.md` when progress is made.

- Keep `PROJECT_TODO.md` simple and easy to scan (flat list or short sections).

This section remains intentionally empty.

---

## 9. Notes for Maintainers

- AI is allowed to update and maintain the development environment (devcontainer, CI, Clippy, rustfmt, justfile, etc.).

- Large-scale architectural changes should be decided by the human maintainer before AI implements them.

- Additional rules or clarifications may be added to this section as needed by maintainers.
