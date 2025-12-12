# Development Workflow

This document describes the development workflow, coding rules, and documentation
conventions for the `kitu-logic-processor` repository.

The goal is to provide a predictable and consistent experience for both human
contributors and AI assistants.


## Table of Contents
- [Tooling and Environment](#tooling-and-environment)
- [Coding Guidelines (Rust)](#coding-guidelines-rust)
- [Documentation Conventions](#documentation-conventions)
- [Development Workflow](#development-workflow)
- [Documents under `doc/`](#documents-under-doc)
- [AI Assistants](#ai-assistants)

## Tooling and Environment

- Development is expected to run inside the Dev Container (`.devcontainer/devcontainer.json`).
- Rust toolchain is pinned via `rust-toolchain.toml`.
- Core tooling:
  - `cargo` (build, test, doc)
  - `clippy` (linting)
  - `rustfmt` (formatting)
  - `just` or `Makefile` (common commands)

Recommended commands (to be provided via `justfile` or equivalent):

- `just fmt` → `cargo fmt --all`
- `just lint` → `cargo clippy --all-targets --all-features -- -D warnings`
- `just test` → `cargo test --all`
- `just check-all` → run fmt, clippy, and tests


## Coding Guidelines (Rust)

- Use **edition 2021** (or later as defined in `Cargo.toml`).
- `rustfmt` is the **single source of truth** for formatting.
- `clippy` warnings should be treated as errors in CI:
  - `cargo clippy --all-targets --all-features -- -D warnings`
- Error handling:
  - Prefer `thiserror` for long-lived library error types.
  - `anyhow::Result` is acceptable at CLI entrypoints.
- Logging and diagnostics:
  - Prefer the `tracing` ecosystem for structured logging.
  - Avoid `println!` in library code (except for examples or clearly-marked debugging).

### Public API and Testing

For any crate or library in this repository:

- Every **public API** (public functions, types, traits, and methods) **MUST** have
  at least one unit test or integration test that exercises its intended behavior.
- If an exception is necessary, it should be clearly documented in code comments or
  relevant docs (e.g., `dev-workflow.md`).

Tests may live either:

- Next to the code in `#[cfg(test)]` modules, or
- In separate integration tests under the `tests/` directory.

Choose whichever makes the intent clearer, but keep the structure consistent within
each crate.


## Documentation Conventions

Rust documentation should follow these conventions for all public items.

### General Rules

- Every `pub` item (functions, structs, enums, traits, methods) **MUST** have a `///` doc comment.
- The comment should follow this structure:
  - First line: a one-sentence summary of what the item does.
  - Following lines: additional details or constraints.
  - Optional sections:
    - `# Examples` — usage examples, preferably doctests.
    - `# Errors` — when and why the function returns `Err`.
    - `# Panics` — conditions under which the function may panic.
    - `# Safety` — required for any `unsafe` function, explaining preconditions and invariants.

Crate- and module-level documentation:

- Each crate must have a crate-level `//!` doc in `lib.rs` explaining:
  - What the crate is for.
  - Its main responsibilities.
  - Pointers to important modules and `doc/` documents.
- Each public module should have a `//!` doc at the top of the module entry file.

### Doctests and Examples

- When adding or changing a public API, add at least one example in a `# Examples` section.
- Wherever possible, examples **SHOULD** compile and run as doctests.
- Larger and more complete examples can live under `examples/`, with a shorter
  variant embedded in the doc comment.

### Lints for Documentation (Optional but Recommended)

Once the crate structure stabilizes, consider enabling:

```rust
#![deny(missing_docs)]
#![warn(rustdoc::broken_intra_doc_links)]
```

in `src/lib.rs` to ensure that:

- All public items are documented.
- Intra-doc links remain valid.


## Development Workflow

The standard workflow for making changes is:

1. Open the repository in VSCode using the Dev Container.
2. Implement or modify code and tests.
3. Ensure everything passes locally:
   - `cargo fmt --all`
   - `cargo clippy --all-targets --all-features -- -D warnings`
   - `cargo test --all`
4. Update documentation where appropriate:
   - Rust doc comments for public APIs.
   - Design and process docs under `doc/` (e.g., `architecture.md`, `testing-strategy.md`).

CI should run the same set of checks (fmt, clippy, tests), plus additional checks
if needed (e.g., `cargo doc` for documentation builds).


## Documents under `doc/`

The `doc/` directory is the main place for higher-level design and workflow documents.
Examples include:

- `architecture.md` — overall architecture and module relationships.
- `testing-strategy.md` — how we approach testing (unit, integration, scenario, replay).
- `protocol-osc-ir.md` — OSC/IR protocol specifications.
- `PRODUCTION_STRATEGY.md` — foundational setup and repository-wide practices.
- `rust-doc-templates.md` — templates for Rust documentation (this file is referenced by that).

Conventions:

- English is the primary language for all documents.
- When a Japanese version is provided (e.g., `*_JP.md`), it should be kept as close
  as reasonably possible to the English source, but English is the source of truth.


## AI Assistants

When AI assistants contribute to this repository, they should:

- Follow the guidelines in this document for coding, testing, and documentation.
- Use `doc/rust-doc-templates.md` for concrete doc-comment patterns.
- Update `doc/` files when implementation or design changes make them outdated.
- Prefer small, focused changes with clear intent and accompanying tests.

This document may evolve over time as the project matures. When workflows or rules
change, update this file accordingly.
