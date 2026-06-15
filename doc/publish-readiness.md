# Publish Readiness Checklist

This document tracks crates.io readiness without permitting actual publication before MVP completion.
The authoritative policy is in [`AGENT.md`](../AGENT.md#cratesio-publish-ready-maintenance-policy-no-publish-until-mvp-completion).

## Policy summary

- Do not run `cargo publish` without `--dry-run`.
- Keep publishable crates ready for future publication while `publish = false` remains in force.
- Treat workspace tools as non-publishable unless a future issue explicitly changes that decision.
- Capture `cargo publish --dry-run` results in the relevant PR or issue before marking dry-run validation complete.

## Candidate crates

Current future-publication candidates are the reusable crates under `crates/`.

| Crate | Candidate | Metadata | `include` | README | Dry-run status |
| --- | --- | --- | --- | --- | --- |
| `kitu-core` | yes | present | present | present | pending #51 |
| `kitu-ecs` | yes | present | present | present | pending #51 |
| `kitu-osc-ir` | yes | present | present | present | pending #51 |
| `kitu-transport` | yes | present | present | present | pending #51 |
| `kitu-runtime` | yes | present | present | present | pending #51 |
| `kitu-app-actions` | yes | present | present | present | pending #51 |
| `kitu-osc-ir-wasm` | yes | pending #50 | pending #50 | present | pending #51 |
| `kitu-scripting-rhai` | yes | present | present | present | pending #51 |
| `kitu-data-tmd` | yes | present | present | present | pending #51 |
| `kitu-data-sqlite` | yes | present | present | present | pending #51 |
| `kitu-tsq1` | yes | present | present | present | pending #51 |
| `kitu-shell` | yes | present | present | present | pending #51 |
| `kitu-web-admin-backend` | yes | present | present | present | pending #51 |
| `kitu-unity-ffi` | yes | present | present | present | pending #51 |

## Non-candidate workspace tools

These packages are workspace utilities or demos and should remain `publish = false` unless a future issue changes their role:

- `tools/kitu-cli`
- `tools/kitu-replay-runner`
- `tools/kitu-web-admin/backend`

If any tool becomes a future crates.io candidate, add full package metadata, add an explicit `include`, and move it into the candidate table above.

## Per-crate checklist

For each candidate crate before MVP publication:

- `Cargo.toml` has `description`, `license` or `license-file`, `repository`, `readme`, and an explicit `include`.
- `README.md` exists and matches the package `readme`.
- Crate-level docs explain responsibility boundaries and integration with the workspace.
- Public APIs have tests or a documented exception.
- `cargo publish --dry-run -p <crate>` has been executed after metadata changes and the result is captured in the PR or issue.
