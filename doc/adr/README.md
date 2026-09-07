# Architecture Decision Records

This directory records accepted decisions that constrain multiple Kitu crates,
hosts, or transports. [`doc/architecture.md`](../architecture.md) remains the
current architecture source of truth, and documents under
[`doc/specs/`](../specs/) remain normative for detailed protocols and runtime
behavior. ADRs preserve the context, alternatives, and consequences behind
those contracts.

The initial records are retrospective. Their decision dates identify when the
decision became established in merged documentation and implementation. They
were recorded as ADRs on 2026-08-18.

## Status lifecycle

- **Proposed**: under discussion and not yet an implementation constraint.
- **Accepted**: implementations and specifications are expected to preserve the
  decision.
- **Deprecated**: retained for history but no longer recommended for new work.
- **Superseded**: replaced by a linked later ADR.

Accepted records are immutable except for typo and link corrections and the
lifecycle metadata needed to deprecate or supersede them. Replace a decision by
adding a new ADR, changing the old record's status to **Superseded**, and linking
the two records without rewriting the old decision. New records start from
[`template.md`](template.md), use the next four-digit number, and distinguish
implemented behavior from staged architecture.

## Index

| ADR | Status | Decision |
| --- | --- | --- |
| [0001](0001-keep-authoritative-gameplay-in-the-rust-runtime.md) | Accepted | Keep authoritative gameplay in the Rust runtime |
| [0002](0002-use-a-fixed-step-commit-and-publish-tick-pipeline.md) | Accepted | Use a fixed-step commit-and-publish tick pipeline |
| [0003](0003-keep-osc-ir-semantics-independent-of-wire-transport.md) | Accepted | Keep OSC-IR semantics independent of wire transport |
| [0004](0004-keep-webtransport-behind-an-experimental-edge-gateway.md) | Accepted | Keep WebTransport behind an experimental edge gateway |
| [0005](0005-separate-reusable-framework-crates-from-applications.md) | Superseded in part by 0007 | Separate reusable framework crates from applications |
| [0006](0006-replay-ordered-inputs-through-the-runtime-boundary.md) | Accepted | Replay ordered inputs through the runtime boundary |
| [0007](0007-distribute-engine-demo-applications-independently.md) | Accepted | Distribute engine demo applications independently |
