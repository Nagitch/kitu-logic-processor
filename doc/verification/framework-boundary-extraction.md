# Framework boundary extraction verification

Date: 2026-09-08. Base: `65e4eec249b20604e232832e227458d4dad24e1f`.
This report covers the shared-boundary change while the application still lives
inside Kitu. It does not claim completion of the subsequent repository migration.

## Environment and checks

macOS ARM64, Rust/Cargo 1.96.0, Node 24.19.0, pnpm 11.9.0, Python 3.14.0,
Xcode SDK 26.5. Rust verification used `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, and `CARGO_PROFILE_TEST_DEBUG=0`.

| Check | Result |
| --- | --- |
| Frozen Arena oracle | Passed: preparation 28 ticks; stock 5528 ticks; expected values unchanged |
| `verify-repository.py --scope test` | Passed: 366 tests in 59 suites, no failures or ignored tests |
| Repository fmt, clippy, docs, data scopes | Passed; 75 Python tooling tests and packaged-data/native interoperability |
| `verify-arena-macos.py --scope native` | Passed: Rust/native scenarios, packaged oracle, wire parity and external C ABI caller |
| Shared Admin package | Frozen install, package build/check and 10 regression tests passed |
| Arena Admin | Check, formatting/lint, 19 inspection tests and production build passed |
| Minimal Admin starter | Check, WASM generation and production build passed |
| Production browser | Arena Overview, Inspector and Game Parameters rendered with a live WS connection; generated WASM initialized without errors |
| Independent package consumer | Installed only package artifacts (`package.json`, `dist`, optional `public`, `LICENSE`), built and served at both `/` and `/review`; CSS rendered, Shell navigation stayed on the correct origin/base, JS and WASM returned HTTP 200, WS opened and no browser errors occurred |

The independent consumer intentionally has no imports from the framework source
tree. Its configured production base path is a verification fixture, not a change
to either checked-in application's deployment URL. Three.js bundle size warnings
remain non-failing build warnings. Fresh CI also exercised the starter's WASM
prebuild with no sibling package binary on PATH; the package resolves its own
declared wasm-pack dependency. Browser verification caught and fixed an empty
base-path regression before the final `/` and `/review` checks above.

## Build identity

Real Cargo builds of the extracted application layout with two independently
relocated Kitu checkouts produced the same execution source hash:
`683762c319b57ed834b94d57b802c9b884ff8edb3d9cfcb9e2d3f9be6c5dc90a`.
Editing a Kitu Rust source file without changing its Git HEAD changed the hash to
`4cf503821152ff28cbebfd079b5950a5f107303ad1b78f57778f467920d9de78`.
Stale manifest and lock inputs were rejected; preparing inputs again succeeded.
These values describe those isolated validation copies, not a promised hash for
all subsequent application commits or build options.

## Baseline issue found during verification

The original `arena_wire` test helper could assign the same temporary directory
to parallel tests. The unmodified baseline failed 30/30 repeated parallel runs;
the sequential suite passed. Adding a process-local atomic counter and exclusive
directory creation fixed the helper: 30/30 parallel runs (270 tests) passed.
This is a test-only change and does not change game behavior or oracle values.

Historical stage-18 runtime artifacts were checked without modification: all
eight archived binary/recording hashes and 686 packaged Player file hashes match
their existing records. Replay rejection by the new independent build and full
licensed Unity/Player verification remain gates of the later migration phase.
