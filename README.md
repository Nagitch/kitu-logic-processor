# Kitu Logic Processor

Kitu provides an authoritative Rust runtime for game logic, ordered input and
publication, typed application actions, transport and replay integration. Engine
clients own presentation and input; applications choose their rules and content.

## Repository boundary

This repository contains reusable crates and tools. The playable Endless Arena
application, Unity project, application Admin and content now live in
[kitu-unity-demo-game](https://github.com/Nagitch/kitu-unity-demo-game).
The [workspace](https://github.com/Nagitch/kitu-workspace) pins a tested combination
of the framework and its independently versioned consumers.

- `crates/`: runtime, ECS, typed data/calculation integrations, OSC-IR, application
  actions, Rhai/TSQ1 adapters, transport, Shell, Admin backend and generic FFI.
- `tools/kitu-web-admin/package/`: reusable `@kitu/admin` Svelte package.
- `tools/kitu-web-admin/starter/`: minimal application demonstrating its public API.
- `tools/kitu-cli/`: reusable CLI for a running host.
- `tools/kitu-replay-runner/`: framework runtime-boundary scenario runner.
- `tools/kitu-webtransport-gateway/`: transport-only experimental edge gateway.
- `kitu-integration-runner/scenarios/smoke/`: application-independent fixtures.
- `doc/`: framework architecture, contracts and decisions.

Applications supply runtime construction, action manifests, gameplay content,
server/embedded hosts and engine-specific clients. Framework crates must not
import application implementations or require their fixtures to compile or test.
[ADR 0007](doc/adr/0007-distribute-engine-demo-applications-independently.md)
records the repository split and its compatibility contract.

## Development and verification

Use the pinned Rust **1.96.0** toolchain and Python **3.11 or later**. Shared Admin
requires Node **24**, pnpm **11.9.0** and the Rust `wasm32-unknown-unknown` target.

```sh
cargo test --locked --workspace
python3 tools/verify-repository.py --scope all --evidence .tmp/framework-1
```

The framework verification runs without downloading the Unity demo. Its Admin
scope builds the shared package and minimal starter using their own lockfiles.
[Web Admin](tools/kitu-web-admin/README.md) documents the public component/client
API, styling, WASM serving and template usage.

A separate compatibility workflow checks a fixed demo revision against the Kitu
PR being tested. Rust, shared Admin, WASM and native package checks are switched
together using the demo's explicit source override. `tools/demo-compatibility.json`
pins the test application; it is not an automatic branch-following dependency.
Framework CI does not claim hosted Unity or application macOS acceptance.
Licensed Unity/Player and macOS native acceptance belong to the demo's own
verification workflow and are not implied by framework CI passing.

## Contracts and architecture

- [Architecture](doc/architecture.md) and [crate ownership](doc/crates-overview.md)
- [Runtime execution](doc/specs/runtime-execution-contract.md)
- [OSC addressing](doc/specs/osc-addressing.md) and [transport envelope](doc/specs/transport-envelope.md)
- [Replay framework](doc/specs/integration-replay-framework.md)
- [Shell](doc/specs/live-shell.md)
- [Architecture decisions](doc/adr/README.md)
- [Development workflow](doc/dev-workflow.md)

Application-specific Arena specifications and historical verification evidence
moved with the demo. Existing Issues, PRs, release tags and earlier commits keep
their original URLs and meaning. The split does not rewrite release history or
change ABI/network identifiers.
