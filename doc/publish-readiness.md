# Publish Readiness Checklist

This document tracks crates.io readiness without permitting actual publication before MVP completion.
The authoritative policy is in [`AGENT.md`](../AGENT.md#cratesio-publish-ready-maintenance-policy-no-publish-until-mvp-completion).

## Policy summary

- Do not run `cargo publish` without `--dry-run`.
- Keep publishable crates ready for future publication while `publish = false` remains in force.
- Treat workspace tools as non-publishable unless a future issue explicitly changes that decision.
- While `publish = false` remains in force, use `cargo package --list` to validate package include scope.
- Run and capture `cargo publish --dry-run` only after a deliberate MVP publication step changes the relevant crate away from `publish = false`.

## Candidate crates

Current future-publication candidates are the reusable crates under `crates/`.

| Crate | Candidate | Metadata | `include` | README | Package validation |
| --- | --- | --- | --- | --- | --- |
| `kitu-core` | yes | present | present | present | pending package list |
| `kitu-ecs` | yes | present | present | present | pending package list |
| `kitu-osc-ir` | yes | present | present | present | pending package list |
| `kitu-transport` | yes | present | present | present | pending package list |
| `kitu-runtime` | yes | present | present | present | pending package list |
| `kitu-app-actions` | yes | present | present | present | pending package list |
| `kitu-osc-ir-wasm` | yes | pending #50 | pending #50 | present | pending package list |
| `kitu-scripting-rhai` | yes | present | present | present | pending package list |
| `kitu-data-tmd` | yes | present | present | present | pending package list |
| `kitu-data-sqlite` | yes | present | present | present | pending package list |
| `kitu-tsq1` | yes | present | present | present | pending package list |
| `kitu-shell` | yes | present | present | present | pending package list |
| `kitu-web-admin-backend` | yes | present | present | present | pending package list |
| `kitu-unity-ffi` | yes | present | present | present | pending package list |

## Non-candidate workspace tools

These packages are workspace utilities or demos and should remain `publish = false` unless a future issue changes their role:

- `tools/kitu-cli`
- `tools/kitu-replay-runner`
- `apps/demo-game`

If any tool or app becomes a future crates.io candidate, add full package metadata, add an explicit `include`, and move it into the candidate table above.

## Per-crate checklist

For each candidate crate before MVP publication:

- `Cargo.toml` has `description`, `license` or `license-file`, `repository`, `readme`, and an explicit `include`.
- `README.md` exists and matches the package `readme`.
- Crate-level docs explain responsibility boundaries and integration with the workspace.
- Public APIs have tests or a documented exception.
- `cargo package --list -p <crate>` has been executed after metadata changes and the result is captured in the PR or issue.
- `cargo publish --dry-run -p <crate>` is deferred until the project intentionally changes the crate's `publish = false` gate as part of MVP publication.
