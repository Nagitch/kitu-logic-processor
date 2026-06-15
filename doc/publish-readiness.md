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
| `kitu-core` | yes | present | present | present | package list succeeded 2026-06-15 |
| `kitu-ecs` | yes | present | present | present | package list succeeded 2026-06-15 |
| `kitu-osc-ir` | yes | present | present | present | package list succeeded 2026-06-15 |
| `kitu-transport` | yes | present | present | present | package list succeeded 2026-06-15 |
| `kitu-runtime` | yes | present | present | present | package list succeeded 2026-06-15 |
| `kitu-app-actions` | yes | present | present | present | package list succeeded 2026-06-15 |
| `kitu-osc-ir-wasm` | yes | present | present | present | package list succeeded 2026-06-15 |
| `kitu-scripting-rhai` | yes | present | present | present | package list succeeded 2026-06-15 |
| `kitu-data-tmd` | yes | present | present | present | package list succeeded 2026-06-15 |
| `kitu-data-sqlite` | yes | present | present | present | package list succeeded 2026-06-15 |
| `kitu-tsq1` | yes | present | present | present | package list succeeded 2026-06-15 |
| `kitu-shell` | yes | present | present | present | package list succeeded 2026-06-15 |
| `kitu-web-admin-backend` | yes | present | present | present | package list succeeded 2026-06-15 |
| `kitu-unity-ffi` | yes | present | present | present | package list succeeded 2026-06-15 |

## Package validation log

Validation was run from the devcontainer on 2026-06-15.

Command template:

```sh
cargo publish --dry-run -p <crate>
cargo package --list -p <crate>
```

Result:

- Every candidate crate returned the same dry-run blocker: `package.publish` must be set to `true` or a non-empty list in `Cargo.toml`.
- This blocker is expected while the repository-wide no-publish policy keeps crates at `publish = false`.
- `cargo package --list` succeeded for every candidate crate, confirming the explicit package include scope can be enumerated.
- Re-run `cargo publish --dry-run -p <crate>` after the project intentionally changes the relevant crate from `publish = false` as part of the MVP publication process.

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
- `cargo package --list -p <crate>` has been executed after metadata changes and the result is captured in the PR or issue.
- `cargo publish --dry-run -p <crate>` is deferred until the project intentionally changes the crate's `publish = false` gate as part of MVP publication.
