# 0005: Separate reusable framework crates from applications

- Status: Superseded for repository placement by [ADR 0007](0007-distribute-engine-demo-applications-independently.md); ownership boundaries remain in force
- Decision date: 2026-06-16

## Context

The repository contains reusable runtime capabilities and a concrete demo game
used for vertical-slice development. When an application host, project actions,
content, Compose topology, and scenarios live in generic framework or tooling
packages, reusable crates begin to depend on one game's policy and fixtures.
That also makes it unclear whether a feature is an engine capability or an
example application's choice.

## Decision

Keep reusable primitives, runtime behavior, and integration libraries under
`crates/`, and reusable operator/development utilities under `tools/`. Put each
concrete product under `apps/`. Application packages may depend on framework
crates; framework crates must not depend on applications.

The historical reference application was `apps/demo-game`; it is now maintained in the independently versioned [Nagitch/kitu-unity-demo-game](https://github.com/Nagitch/kitu-unity-demo-game) repository. It owns its runtime
construction, project action manifest, admin host binary, Compose stack, and
application-specific scenario fixtures. Keep shared action catalog types and
general actions in `kitu-app-actions`, while loading project-owned definitions
into the runtime at application construction.

## Consequences

- Framework crates can be reused without importing demo-game policy, content,
  servers, or test fixtures.
- A vertical slice can still prove the complete framework through one concrete
  application and its scenario tests.
- Moving behavior into `apps/` makes it intentionally unavailable to other
  applications until extracted behind a reusable contract.
- Changes spanning framework and application layers need separate reasoning
  about which side owns the behavior.
- Application manifests and scenarios can evolve independently while shared
  catalog validation remains centralized.

## Alternatives considered

- **Keep the demo host under Web Admin tools**: convenient initially, but the
  host constructs a game runtime and owns application state rather than a
  reusable frontend tool.
- **Put demo behavior in runtime crates**: simplifies the example, but turns one
  game's actions and content into framework dependencies.
- **Use a separate repository for every app immediately**: enforces isolation,
  but makes coordinated vertical-slice development and CI more expensive at the
  current stage.

## References

- [Application conventions](../../apps/README.md)
- [Demo game repository](https://github.com/Nagitch/kitu-unity-demo-game)
- [`kitu-app-actions`](../../crates/kitu-app-actions/README.md)
- Pull requests [#61](https://github.com/Nagitch/kitu-logic-processor/pull/61)
  and [#77](https://github.com/Nagitch/kitu-logic-processor/pull/77)
