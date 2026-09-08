# 0007: Distribute engine demo applications independently

- Status: Accepted
- Decision date: 2026-09-08
- Supersedes the same-repository application placement in [ADR 0005](0005-separate-reusable-framework-crates-from-applications.md).

## Context

The Unity Endless Arena vertical slice now exercises the complete runtime through
server, embedded native, authoring, inspection and replay flows. The application
is large enough that sharing the framework repository obscures ownership,
particularly where its Admin screens extend reusable operator UI.

## Decision

`Nagitch/kitu-unity-demo-game` owns the Unity project, Rust application and native
factory, content, application Admin, scenarios, build tools and application
documentation. Its name describes the presentation engine; Arena remains the
name of the current game and its existing protocol. A future Unreal demo can be
another peer application without becoming a framework dependency.

`kitu-logic-processor` retains reusable Rust crates, the generic C ABI, operator
tools, protocol fixtures and framework tests. It publishes the source interface
of one `@kitu/admin` package and a minimal consumer starter. Applications own
SvelteKit routes and compose exported components using explicit connection,
navigation, header and object-kind configuration. Application schemas and
application-specific API clients remain in the application.

The application pins one complete Kitu Git revision for Rust, Admin and WASM.
Setup builds the Admin package from that source; registry publication is outside
this migration. An explicit local-source override replaces all three together.
Native libraries are built from the selected source, not reused from a previous
release. Dependency resolution and build reports identify the actual code used.

Framework CI owns ordinary standalone checks and a separate downstream check:
a pinned demo revision is compiled and tested against the framework under test.
The demo owns its own checks against its pinned framework. The parent
`kitu-workspace` stores tested combinations as sibling submodule revisions.
The CI fixture pin and the application's dependency pin have different purposes
and are not required to recursively point at one another's latest commit.

## Compatibility and provenance

- Preserve existing game rules, public message identifiers, ABI symbols, content
  schemas, Unity asset GUIDs and the frozen Unity-only comparison oracle.
- Preserve extracted file history and original-to-extracted commit mapping.
  Original repository history, tags, issues and PRs remain at their original URLs.
- Resolve source locations explicitly when calculating the replay execution
  identity. Hash source contents and resolved dependencies rather than absolute
  checkout paths. Missing provenance is an error.
- New builds have new execution identities. Older recordings continue to require
  their retained matching executables; identity checks must not be weakened to
  make historical recordings appear compatible.

## Delivery

First extract shared interfaces while the existing application still passes.
Then prove the independent demo from a fresh checkout, replace the framework's
in-tree application checks with downstream checks, and update workspace pointers
last. Keep framework and application commits separate. The migration is complete
only after standalone framework, standalone demo, downstream PR, fresh workspace,
and licensed macOS Unity/Player validation have all been evidenced.
