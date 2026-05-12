---
name: Work item
about: Define one scoped unit of work for implementation, documentation, verification, or design decisions.
title: ""
labels: ""
assignees: ""
---

<!--
Use this issue as the single source of truth for one work item.
Keep the scope small enough that one PR can usually close it.
Delete any checklist item that clearly does not apply.
-->

## Summary

<!-- What should change, in one or two sentences? -->


## Why

<!--
Describe the reason this work is needed.
Link related specs, docs, PRs, issues, failing tests, or observed gaps.
-->


## Scope

<!-- What is included in this issue? -->

- 

## Out of Scope

<!-- What should intentionally not be changed in this issue? -->

- 

## Definition of Done

<!--
The issue is not complete until these conditions are satisfied.
Prefer observable outcomes over vague implementation notes.
-->

- [ ] The requested behavior, document change, or decision is implemented.
- [ ] Tests or verification evidence cover the changed behavior when applicable.
- [ ] Public API changes include Rust documentation and examples when applicable.
- [ ] Architecture, spec, README, or crate documentation is updated when behavior or workflow changes.
- [ ] No known contradiction remains between implementation, specs, and docs.

## Verification

<!--
List the exact commands, manual checks, replay runs, screenshots, or review evidence
that should prove the work is complete.
-->

- [ ] `cargo fmt --all`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test --all`

Additional verification:

- 

## Expected Files or Areas

<!--
List likely files, crates, specs, or docs.
This is a planning hint, not a restriction.
-->

- 

## Notes for Codex

<!--
Add implementation constraints, preferred approach, risks, or sequencing notes.
Codex should update this issue or the linked PR description if the scope changes.
-->

- Prefer a focused PR that closes this issue.
- Preserve existing behavior unless this issue explicitly changes it.
- Update English source documents first; update Japanese paired documents when they exist and the change affects them.

