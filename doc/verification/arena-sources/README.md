# SQLite and layered Arena verification

[Stage 12 / Issue 152](https://github.com/Nagitch/kitu-logic-processor/issues/152)
was verified on 2026-09-06 with real SQLite, the pinned Tanu public API, the shared
server/native Runtime, actual CLI/Admin and Unity 6000.6.0f1. The frozen C# source,
input fixtures and comparison tolerances remain unchanged.

| Check | Result | Evidence |
|---|---|---|
| Dev Container Rust | 250 tests/doctests; format, Clippy and rustdoc passed | [Results](results.json) |
| SQLite semantics | Native types, ordering, bounds, cancellation, WAL changes and concurrent snapshot consistency | `crates/kitu-data-sqlite/src/tests.rs` |
| Arena source contract | Mixed sources, four-layer precedence, strict patches, origins, stale tokens, detached stock replay after source deletion | `apps/demo-game/tests/arena_sources.rs` and host/native tests |
| Actual Admin | Active 20 retained while staging 32; next start adopted Debug 32, then SQLite Event 37; invalid -1 retained 37; recovered 39 stayed unapplied | [Browser](browser.json) |
| Actual CLI replay | Original Debug 32 recording verified and sought after the authoring stack changed to Event 39; saved origins remained Debug; live active/pending 37 retained | [CLI](cli.json) |
| Frontend | Typecheck 0/0, lint and full production WASM/Vite build passed | [Results](results.json), [initial frontend checks](frontend.json) |
| macOS native | 12 tests/doctests, including actual mixed-source HTTP staging and replay with deleted sources | [Validation summary](validation-summary.json) |
| Actual C caller | Complete state/output equality: preparation 28 ticks / 40 inputs; stock 5,528 / 5,581 | [Preparation](preparation-comparison.json), [stock](stock-comparison.json) |
| Unity | 50 EditMode and 21 PlayMode tests, no failures/skips | [Unity tests](unity-tests.json) |
| Graphical standalone | Preparation and entire stock run matched every native state/output; six rendered checkpoints visually checked | [Preparation](player-preparation.json), [stock](player-stock.json), [visual QA](visual-qa.json) |
| Native/player packaging | Actual ARM64 library and Mono Player; matching bundled code and signatures, 0 build errors / 4 existing warnings | [Native package](native-package.json), [Player build](player-build.json) |

PR review exposed source tables that shadowed SQLite's table-valued metadata
pragmas. Two regressions failed before the fix; direct PRAGMA statements now
inspect the real schema. The [review regression](review-regression.json) also
records an actual authoring CLI rejection of a forged source inventory and
unchanged typed values/hashes/provenance for the legitimate layered candidate.
All Rust checks, native tests, C comparisons, native packaging, Player build and
complete graphical fixtures were refreshed after this fix. The unchanged Unity
and browser/CLI suites retain their earlier results; the six final Player PNGs
are byte-identical to the visually inspected originals.

The browser used four generated source files. TMD/SQLite layers applied in fixed
order, and removing the Debug slot exposed the edited SQLite Event value. Candidate
validation/staging preserved active values until start. Invalid edits cleared
candidate sources/differences and preserved active/pending versions. The corrected
development WebSocket setting was `PUBLIC_KITU_ADMIN_WS_URL`; final connection,
navigation and browser error checks passed.

The saved layered recording was re-executed through the actual CLI after those
edits. Step and seek matched its verified state; content inspection showed the
saved Debug origin. A CLI attempt to stage current content during playback was
refused. Returning live preserved the original active/pending/candidate settings,
paused the game, and accepted explicit resume/pause commands.

Run the [source authoring commands](../../../apps/demo-game/README.md#sqlite-and-layered-parameters),
the [native ABI checks](../../specs/arena-native-abi.md) and the
[macOS build/Player checks](../../../kitu-integration-runner/unity-demo-game/README.md#reproduce-the-embedded-macos-build)
to reproduce the flow. The source contract explains the [Tanu cancellation and
per-file transaction limits](../../specs/arena-content-sources.md).

These compact generated reports retain local paths and artifact hashes. Stage12
PNGs, full traces, Unity XML/logs, native libraries and the `.app` remain local.
The exact tested Linux and macOS binaries and Player were archived before later
execution changes, so old recordings can continue using their matching build.
The nine separately authorized original Stage11 screenshots are published in
[the preceding verification record](../arena-embedded/README.md).
