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

## crates.io Publish-Ready Maintenance Policy (No Publish Until MVP Completion)

### Purpose

* Keep all crates in a state that is ready for future publication to crates.io ("publish-ready").
* **Do not publish any crate to crates.io until the MVP is completed.**

This project prioritizes the ability to transition smoothly to crates.io publication after the MVP is complete. Therefore, even during active development, crates must maintain publication-quality metadata, documentation, and structure.

---

### Prohibited Actions Until MVP Completion

Until the MVP is completed, the following actions are strictly prohibited:

* Executing `cargo publish` (except `cargo publish --dry-run`)
* Creating or updating any crate on crates.io
* Creating official releases or release tags that assume crates.io publication

  * Draft releases or internal-only tags are acceptable if needed

---

### Definition of "Publish-Ready"

Until the MVP is completed, each crate must satisfy the following conditions to be considered "publish-ready".

#### 1. Cargo.toml Requirements

Any crate that may eventually be published must have the following configured in its `Cargo.toml`:

* Required `[package]` fields:

  * `name`
  * `version` (Semantic Versioning format)
  * `edition`
  * `description` (a one-line statement clearly describing responsibility)
  * `license` or `license-file`
  * `repository`
  * `readme`
* Recommended fields (when reasonable):

  * `keywords` (up to 5)
  * `categories` (selected from crates.io predefined categories)

To explicitly control packaged files, crates should define `include`:

```toml
include = ["src/**", "Cargo.toml", "README.md", "LICENSE*"]
```

---

#### 2. Documentation Requirements

Each crate must be documented with the assumption that it will be rendered on docs.rs.

* `lib.rs` (or `main.rs`) must contain crate-level documentation (`//!`) at the top, describing:

  * The purpose of the crate
  * What the crate provides
  * What the crate intentionally does *not* provide
  * How it relates to other crates in the workspace
* All public APIs (`pub`) must have documentation comments (`///`)

  * `pub mod` items must also include a module-level overview
* Code examples in documentation should be valid doctests whenever possible

Policy regarding missing documentation:

* Default: `#![warn(missing_docs)]`
* For stable or foundational crates, migrating to `#![deny(missing_docs)]` is encouraged
* If enforcing `deny` significantly slows MVP development, continuing with `warn` is acceptable

---

#### 3. Quality Gates for Maintaining Publish-Ready State

Whenever Codex modifies a crate, the following conditions must be satisfied:

* `cargo fmt --check`
* `cargo clippy -- -D warnings` (if this is impractical, the reason must be documented)
* `cargo test`
* `cargo doc --no-deps`

Although actual publishing is prohibited, packaging integrity **must** be verified using:

```bash
cargo publish --dry-run
```

The dry-run must confirm:

* Only intended files are included in the package
* README and metadata are correctly resolved
* The crate satisfies crates.io publication requirements

---

#### 4. Handling Non-Publishable Crates

For crates that are internal-only or not intended for crates.io publication in the near term, one of the following must be explicitly declared:

* Set `[package] publish = false`
* Treat the crate as publish-ready, including README and LICENSE, even if publication is deferred

If `cargo publish --dry-run` fails due to workspace structure or feature dependencies, the failure reason and mitigation plan must be recorded as TODOs. Large refactors that would block MVP progress should be avoided.

---

### Codex Operational Guidelines

* Each change set (PR or task) should focus on one of the following:

  * Making a single crate publish-ready
  * Applying a publish-ready rule consistently across multiple crates
* Each change must document:

  * Which crates are considered publish-ready
  * The result of `cargo publish --dry-run` (success or failure with explanation)

---

### Final Objective

* At the point of MVP completion, all target crates can be published to crates.io without additional large-scale cleanup
* Until that point, **no actual publication must be performed**



## 9. Notes for Maintainers

- AI may maintain development tooling and documentation.
- Large architectural decisions should be approved by the human maintainer.
- Reference documents required for all contributions:
  - **`doc/rust-doc-templates.md`**
  - **`doc/dev-workflow.md`**
  - **`doc/PRODUCTION_STRATEGY.md`**
- These documents must be updated when workflows or project structure changes.

