# Kitu applications

`apps/` contains applications built with the reusable Kitu framework crates.
Code here may depend on `crates/`, but framework crates should not depend on app
packages.

## Current apps

- `demo-game/`: reference application for vertical-slice development, Web Admin
  hosting, and CI scenario tests.

## Conventions

- Put app-specific manifests, scenarios, data, and host binaries under the app
  directory.
- Keep reusable engine/runtime/tooling code under `crates/` or `tools/`.
- Prefer app-owned scenario tests when a fixture depends on project actions or
  content.
