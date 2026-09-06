# Endless Arena delivery status

The approved [roadmap #129](https://github.com/Nagitch/kitu-logic-processor/issues/129)
has **stages 1–17 merged**. Stage 18 is in progress in
[Issue 164](https://github.com/Nagitch/kitu-logic-processor/issues/164); its final
checks, PR, review, merge and workspace reference update are pending.
The [machine-readable matrix](stages.json) retains exact merge identities.

Earlier verification pages are historical captures. A page saying “PR/CI pending”
records the state when its artifacts were captured, even if that stage has since
merged. The table below records verified merge status; its links retain the
original measurements and review evidence. Historical passes are not substitutes
for a current verification run.

## Stages

| Stage | Delivered scope | Issue / merged PR | Contract | Evidence |
| --- | --- | --- | --- | --- |
| 1 | Contract and preserved Unity oracle | [#130](https://github.com/Nagitch/kitu-logic-processor/issues/130) / [PR 131](https://github.com/Nagitch/kitu-logic-processor/pull/131) | [Specification](../../specs/arena-runtime-contract.md) | [Record](../arena-reference/results.json) |
| 2 | Independent Runtime and Unity connection | [#132](https://github.com/Nagitch/kitu-logic-processor/issues/132) / [PR 133](https://github.com/Nagitch/kitu-logic-processor/pull/133) | [Specification](../../specs/arena-runtime-contract.md) | [Record](../arena-runtime-host/results.json) |
| 3 | Inventory, equipment, HP and shields | [#134](https://github.com/Nagitch/kitu-logic-processor/issues/134) / [PR 135](https://github.com/Nagitch/kitu-logic-processor/pull/135) | [Specification](../../specs/arena-runtime-contract.md) | [Record](../arena-inventory/results.json) |
| 4 | Combat, AI, projectiles and death | [#136](https://github.com/Nagitch/kitu-logic-processor/issues/136) / [PR 137](https://github.com/Nagitch/kitu-logic-processor/pull/137) | [Specification](../../specs/arena-runtime-contract.md) | [Record](../arena-combat/results.json) |
| 5 | Progression, bosses, results and retry | [#138](https://github.com/Nagitch/kitu-logic-processor/issues/138) / [PR 139](https://github.com/Nagitch/kitu-logic-processor/pull/139) | [Specification](../../specs/arena-runtime-contract.md) | [Record](../arena-progression/results.json) |
| 6 | Tanu tables and next-run parameters | [#140](https://github.com/Nagitch/kitu-logic-processor/issues/140) / [PR 141](https://github.com/Nagitch/kitu-logic-processor/pull/141) | [Specification](../../specs/arena-content-sources.md) | [Record](../arena-tanu/results.json) |
| 7 | TSQ1 recording and re-execution | [#142](https://github.com/Nagitch/kitu-logic-processor/issues/142) / [PR 143](https://github.com/Nagitch/kitu-logic-processor/pull/143) | [Specification](../../specs/arena-replay.md) | [Record](../arena-tsq1/results.json) |
| 8 | Admin playback and seek | [#144](https://github.com/Nagitch/kitu-logic-processor/issues/144) / [PR 145](https://github.com/Nagitch/kitu-logic-processor/pull/145) | [Specification](../../specs/arena-replay.md) | [Record](../arena-playback/results.json) |
| 9 | Live CLI and browser Shell | [#146](https://github.com/Nagitch/kitu-logic-processor/issues/146) / [PR 147](https://github.com/Nagitch/kitu-logic-processor/pull/147) | [Specification](../../specs/live-shell.md) | [Record](../arena-shell/results.json) |
| 10 | Full application C ABI | [#148](https://github.com/Nagitch/kitu-logic-processor/issues/148) / [PR 149](https://github.com/Nagitch/kitu-logic-processor/pull/149) | [Specification](../../specs/arena-native-abi.md) | [Record](../arena-native/results.json) |
| 11 | Embedded macOS Player and bridge | [#150](https://github.com/Nagitch/kitu-logic-processor/issues/150) / [PR 151](https://github.com/Nagitch/kitu-logic-processor/pull/151) | [Specification](../../specs/arena-embedded-host.md) | [Record](../arena-embedded/README.md) |
| 12 | SQLite and layered content | [#152](https://github.com/Nagitch/kitu-logic-processor/issues/152) / [PR 153](https://github.com/Nagitch/kitu-logic-processor/pull/153) | [Specification](../../specs/arena-content-sources.md) | [Record](../arena-sources/README.md) |
| 13 | Bounded Rhai boss rules | [#154](https://github.com/Nagitch/kitu-logic-processor/issues/154) / [PR 155](https://github.com/Nagitch/kitu-logic-processor/pull/155) | [Specification](../../specs/arena-boss-scripts.md) | [Record](../arena-scripts/README.md) |
| 14 | TSQ1 presentation timelines | [#156](https://github.com/Nagitch/kitu-logic-processor/issues/156) / [PR 157](https://github.com/Nagitch/kitu-logic-processor/pull/157) | [Specification](../../specs/arena-presentation-timelines.md) | [Record](../arena-timelines/README.md) |
| 15 | JSON, MessagePack and compatibility | [#158](https://github.com/Nagitch/kitu-logic-processor/issues/158) / [PR 159](https://github.com/Nagitch/kitu-logic-processor/pull/159) | [Specification](../../specs/arena-application-wire.md) | [Record](../arena-wire/README.md) |
| 16 | Addressables and bundled content | [#160](https://github.com/Nagitch/kitu-logic-processor/issues/160) / [PR 161](https://github.com/Nagitch/kitu-logic-processor/pull/161) | [Specification](../../specs/arena-packaged-content.md) | [Record](../arena-content/README.md) |
| 17 | Coherent Admin inspection | [#162](https://github.com/Nagitch/kitu-logic-processor/issues/162) / [PR 163](https://github.com/Nagitch/kitu-logic-processor/pull/163) | [Specification](../../specs/arena-inspection.md) | [Record](../arena-inspection/README.md) |
| 18 | Reproducible builds, CI and delivery status | [#164](https://github.com/Nagitch/kitu-logic-processor/issues/164) / PR pending | [Specification](../../specs/arena-build-verification.md) | [Record](../arena-delivery/README.md) |

The latest merged parent workspace reference is
[PR 39](https://github.com/Nagitch/kitu-workspace/pull/39), which includes Kitu
[PR 163](https://github.com/Nagitch/kitu-logic-processor/pull/163). Dependency
repositories keep their own history; parent workspace updates follow child
merges. No new Tanu or TSQ1 repository change is required for this build stage.

## What the reference application demonstrates

- The [Unity-only baseline](../../specs/unity-only-arena-game.md) from
  [PR 128](https://github.com/Nagitch/kitu-logic-processor/pull/128), investigated
  in [Issue 111](https://github.com/Nagitch/kitu-logic-processor/issues/111), and frozen C#
  source/input fixtures remain available. Discrete state, event ticks and
  hit/death outcomes must agree; initial floating-state tolerance is `1e-4`
  absolute. Current native/socket/Player verifiers compare complete logical
  state and ordered outputs. Pixel equality is not the gameplay contract.
- The stock input sequence reaches 11F, dies naturally and retries. Separate
  progression checks cover rules through 21F. Server, native C ABI, TSQ1 replay
  and the graphical macOS standalone have actual execution evidence above.
- Tanu/SQLite settings, bounded Rhai boss rules and real TSQ1 clips are validated
  before next-run adoption. Saved recordings retain detached values and exact
  source versions; edits do not change a running or recorded game.
- CLI and browser Shell use shared live commands. Admin inspects the selected
  server or embedded Runtime and controls exact replay positions. Inspector
  timing measures host owner-update cost; no performance threshold or FPS
  guarantee is claimed.
- The macOS Player contains the native library, TMD/Rhai/TSQ1 source package and
  local Addressables material/prefabs. SQLite is supported as an authoring
  source; the default five-file Player source package contains TMD, not SQLite.

Stage 17 local checks passed 360 unique Rust tests/doctests across 58 suites,
19 frontend tests, 143 EditMode, 31 MessagePack PlayMode and seven JSON network
PlayMode cases. Its CI ran 361 Rust test/doctest cases because the existing
scenario job repeated one case. These are distinct recorded scopes, not 361
unique tests. See the [Stage 17 records](../arena-inspection/README.md).

## Stage 18 verification

Use the [reproducible build and verification recipe](../../specs/arena-build-verification.md)
for repository checks in the Dev Container and native/full macOS checks. The
coordinators reuse existing scenario, data, C ABI, Unity and Player verifiers.
This stage adds reproducible orchestration and CI coverage; it does not change
the authoritative Arena rules.

| Required completion | Status |
| --- | --- |
| Shared repository checks: reference, Rust, frontend/WASM and data | Pending |
| macOS native tests, C caller, library and source-package proof | Pending |
| Licensed Unity tests and graphical default/edited Player proof | Pending |
| Final CI, review and Kitu PR merge | Pending |
| Parent workspace reference update | Pending |

Final reports will be linked here after execution. Build output, full state and
trace files, recordings, runtime archives and PNGs remain local ignored artifacts;
compact reports retain commands, counts, versions and hashes.

## Limits and later work

The current standalone target is macOS ARM64. Apple SDK/native checks run on
macOS; full Unity checks require the pinned licensed Editor and a usable local
graphical session. Missing SDK, licensing or graphical access is an environment
blocker, reported explicitly rather than counted as a passed or skipped gate.

Windows, multiplayer, production remote authorization/operations, CDN delivery,
cloud deployment, public package releases and advanced future README concepts
are outside this delivery. The embedded development bridge is local tooling,
not a deployed authenticated service. Local Addressables and reproducible build
artifacts do not imply CDN publishing or release signing/notarization.

No in-scope feature has been deferred as structurally impossible. The remaining
Stage 18 checks and merge are pending work, not impossibility claims.
