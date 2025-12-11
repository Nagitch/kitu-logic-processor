# AGENT.md — Kitu Logic Processor

This document explains how AI assistants (e.g., ChatGPT / Codex) should work on this repository.  
It defines development principles, coding standards, documentation rules, and maintenance responsibilities.

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
- AI assistants may:
  - Propose alternative structures.
  - Create missing modules.
  - Iterate on development tooling and CI.

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
  - `rust-doc-templates.md`
  - `PRODUCTION_STRATEGY.md`
- `.devcontainer/`
  - `devcontainer.json`
  - (optional) `Dockerfile`
- `.github/workflows/`
- `AGENT.md`
- `CONTRIBUTING.md`
- `README.md`, `README_JP.md` (optional)

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

### 4.1 Allowed and encouraged tasks

1. **Code implementation**  
   - Write or update Rust modules, traits, and functions.
   - Add new crates, modules, or features when appropriate.

2. **Testing**  
   - Write unit tests and integration tests.
   - Maintain consistent test coverage, especially for public APIs.

3. **Documentation**  
   - Maintain all documents under the `doc/` directory.
   - **English documentation is the source of truth**.
   - Update corresponding Japanese versions (`*_JP.md`) when they exist.
   - Follow the conventions from:
     - **`doc/rust-doc-templates.md`** – templates for doc-comments (`///`, `//!`)
     - **`doc/dev-workflow.md`** – rules for documentation, examples, testing, and doctests
   - When adding or modifying public APIs:
     - Add appropriate Rust documentation using the templates above.
     - Include `# Examples`, preferably as doctests.
     - Update architecture or design documents if behavior changes.

4. **Development environment maintenance**  
   - Maintain `.devcontainer/` configuration.
   - Keep Rust toolchain settings (`rust-toolchain.toml`) up to date.
   - Maintain `clippy.toml`, `rustfmt.toml`, `justfile`.
   - Maintain CI workflows (fmt, clippy, test).

5. **Housekeeping**  
   - Maintain `.gitignore`, `.editorconfig`, repository scripts.

---

## 5. Coding guidelines

- Use Rust edition 2021 or later.
- `rustfmt` is the formatting authority.
- Clippy warnings should be treated as build errors.
- Prefer `thiserror` for error types.
- Use `anyhow` in CLI entrypoints.
- Prefer `tracing` for logging.
- **All public APIs MUST have tests** (unit or integration).
- Follow documentation rules from `doc/dev-workflow.md`.

---

## 6. Development workflow

1. Use the Dev Container for all development work.
2. Before committing:
   - `cargo fmt --all`
   - `cargo clippy --all-targets --all-features -- -D warnings`
   - `cargo test --all`
3. Update documentation as needed:
   - Rust doc-comments
   - Documents under `doc/`

CI should mirror the same checks.

---

## 7. How AI should apply changes

1. Describe the intent before making changes.
2. Perform small, well-scoped updates.
3. Keep this file updated when development processes evolve.
4. Update documents under `doc/` when design or workflow changes.

---

## 8. Open tasks / TODO for AI

Project-level tasks are maintained inside **`PROJECT_TODO.md`**.  
AI assistants must:

- Add new tasks there instead of inside AGENT.md.
- Update or close tasks when work progresses.
- Keep the task list concise and readable.

---

## 9. Notes for Maintainers

- AI may maintain development tooling and documentation.
- Large architectural decisions should be approved by the human maintainer.
- Reference documents required for all contributions:
  - **`doc/rust-doc-templates.md`**
  - **`doc/dev-workflow.md`**
  - **`doc/PRODUCTION_STRATEGY.md`**
- These documents must be updated when workflows or project structure changes.

