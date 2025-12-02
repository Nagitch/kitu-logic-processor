# PRODUCTION_STRATEGY.md

This document summarizes the foundational setup required to streamline ongoing development of the *kitu-logic-processor* repository.
It serves as a reference for establishing consistent development environments, repository structure, and tooling.

---

## 1. Repository Meta

**Essential Components**

- **README.md / README_JP.md**
  - Should include:
    - How to build / run (dev, test)
    - The role of this repository within the broader Kitu ecosystem

- **AGENT.md**
  - The operational guide for ChatGPT / Codex.
  - Should cover:
    - Project overview
    - Goals and long-term milestones
    - Coding conventions (Rust, testing policy, directory rules)
    - Tasks AI should / should not perform
    - Where generated code and documents should be placed (`src/`, `doc/`, `examples/`)

- **CONTRIBUTING.md**
  - Local development steps
  - Commit message conventions (if any)
  - Branching strategy (main / develop / feature-*)

---

## 2. Rust Toolchain & Static Analysis

**Core Rust Setup**

- **rust-toolchain.toml**
  - Pin Rust version (e.g., `channel = "1.82.0"`)

- **Cargo.toml + Workspace Layout**
  - Prepare for multi-crate workspace (e.g., `kitu-core`, `kitu-ecs`, etc.)

- **clippy.toml**
  - Configure preferred linting rules:
    - Example: `warn = ["clippy::all", "clippy::pedantic"]`

- **rustfmt.toml**
  - Formatting rules (`edition = "2021"`, `max_width`, etc.)

**Unified Commands**

- Provide a `justfile` or `Makefile` with commands:
  - `just format` → `cargo fmt --all`
  - `just lint` → `cargo clippy --all-targets --all-features -- -D warnings`
  - `just test` → `cargo test --all`
  - `just check-all` → runs all of the above

---

## 3. Dev Container / VSCode Integration

### `.devcontainer/devcontainer.json`

Minimum setup:

- Base image:
  - `mcr.microsoft.com/devcontainers/rust` or custom Debian + rustup

- Additional packages:
  - `clang`, `lld`, `pkg-config`, `libssl-dev`, `git`, `zsh`, `fzf`, `ripgrep`

- Recommended VSCode extensions:
  - `rust-analyzer`
  - `Even Better TOML`
  - `Error Lens`
  - `GitLens`
  - `EditorConfig`

- Initialization scripts:
  - `rustup component add clippy rustfmt`
  - Optionally: `cargo install just`

### `.vscode/` Settings

- `extensions.json` — recommended extensions
- `settings.json` — Rust Analyzer configuration, formatting rules

---

## 4. CI / Automated Checks

Use GitHub Actions with a workflow such as `rust-ci.yml`:

- Trigger on `push` and `pull_request`
- Jobs include:
  1. `cargo fmt --all --check`
  2. `cargo clippy --all-targets --all-features -- -D warnings`
  3. `cargo test --all`

Optional:
- Build documentation (`cargo doc --no-deps`)
- Add scenario tests for logic execution

---

## 5. Initial Code Skeleton

- `src/lib.rs`
  - Prepare module structure:
    - `mod ecs;`
    - `mod timeline;`
    - `mod scripting;`
    - `mod bridge;`

- `src/bin/`
  - Example CLI entry: `kitu-cli.rs`

- `examples/`
  - Start with minimal example: `examples/minimal_sim.rs`

---

## 6. Documentation Structure

Recommended documents under `doc/`:

- `architecture.md`
  - Expanded architecture overview

- `dev-workflow.md`
  - Rust-side development steps, testing, and Unity integration notes

- `protocol-osc-ir.md`
  - OSC-IR event specifications

- `testing-strategy.md`
  - Testing methodology:
    - Unit tests
    - Scenario / replay tests
    - TSQ1 / TMD data-driven testing

---

## 7. Git Management & Miscellaneous

- `.gitignore` — ignore output (`target/`, logs, temporary files, etc.)
- `.editorconfig` — consistent indent style, newline rules
- Issue/PR templates:
  - `bug_report.md`
  - `feature_request.md`
  - `pull_request_template.md`

---

## 8. AGENT.md Highlights

AGENT.md should explicitly define:

- **High-level goals** (e.g., Rust-based standalone logic core, TSQ1/TMD-driven)
- **Tasks for AI**
  - Generate module skeletons
  - Create tests, benchmarks, documentation
- **Tasks AI should avoid**
  - Copying external code
  - Bringing in unsafe licensing
- **Coding conventions**
  - Error handling, tracing, logging
- **File layout rules**
  - Logic in `src/`
  - Design docs in `doc/`
  - Experiments in `sandbox/`

---

## Summary

To bootstrap the project efficiently, prioritize:

1. **Rust environment setup**  
   (`rust-toolchain.toml`, lint/format configs, justfile)

2. **Dev Container + VSCode integration**  
   (`.devcontainer/devcontainer.json`, `.vscode/settings.json`)

3. **CI pipeline**  
   (fmt / clippy / test)

4. **Meta documents**  
   (`AGENT.md`, `CONTRIBUTING.md`, `doc/dev-workflow.md`)

5. **Minimal Rust skeleton**  
   (`src/lib.rs`, CLI entry, examples/)

With this foundation prepared, subsequent implementation steps become significantly smoother.

