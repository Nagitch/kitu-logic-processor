# Endless Arena delivery status

The approved [roadmap #129](https://github.com/Nagitch/kitu-logic-processor/issues/129)
has **all 18 implementations and required local checks complete in this tree**,
with **stages 1–17 merged**. Stage 18 is tracked in
[Issue 164](https://github.com/Nagitch/kitu-logic-processor/issues/164). Automatic
approval review rejected its GitHub push, and explicit user approval is pending.
No Stage 18 PR has been created; CI, review, merge and the parent reference
update have not run. The parent update is tracked in
[workspace Issue 40](https://github.com/Nagitch/kitu-workspace/issues/40).
The [machine-readable matrix](stages.json) retains exact historical merge
identities and separates local completion from publication.

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
| 18 | Reproducible builds, CI and delivery status | [#164](https://github.com/Nagitch/kitu-logic-processor/issues/164) / publication awaiting approval | [Specification](../../specs/arena-build-verification.md) | [Record](results.json) |

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
| Shared repository checks | Passed: 360 Rust tests/doctests across 58 suites, reference/fmt/Clippy/rustdoc, 19 frontend cases and full WASM/Vite build |
| Final data/coordinator regression scope | Passed separately: 65 portable Python cases and actual Rust/C ABI package interoperability |
| Dev Container and workspace build | Actual image/setup and locked workspace build passed; Git LFS prerequisite verified |
| Complete macOS coordinator | Fresh `macos-full-02` passed all 18 required steps, with cleanup passed and no unexpected changes |
| Native C ABI and Unity | 29 native cases including doctests; four C traces; 143 EditMode, 31 MessagePack and seven JSON PlayMode cases; 11 codec export pairs and coherent inspection checks passed |
| Graphical standalone and content | Four default scenarios passed 9,186 ticks / 9,257 inputs; edited bundled source passed 1,800 ticks / 1,816 inputs with the initial oracle. Relocation, authoring preservation and six invalid-content starts passed |
| Actual Admin browser | Server and relocated embedded Player each passed all 15 flow checks with no page errors |
| Retained execution identities and cleanup | Eight archived artifacts and two signed Players verified; owned processes stopped and ports released |
| GitHub push, CI, review and Kitu PR merge | Push awaits user approval after automatic approval review rejected it; no Stage 18 PR, CI run, review or merge |
| Parent workspace reference update | Not performed; tracked in workspace Issue 40 |

[results.json](results.json) separates completed local checks from pending
publication. [repository.json](repository.json) retains exact commands, counts
and source/lock/log identities. The full Dev Container attempt ran from
`cc75f9320fd5160598a94c55e0ffef4721049f84` with 61 Python cases; the later data
scope ran from clean `7db01caeaa98e6ed353fc4b2aa9526e4c8704b0b` with 65. The
final scope includes the corrected coordinator regressions and package interop;
these repeated focused cases are not added to the 360 unique workspace total.
Tests use `--locked --workspace`, while Clippy and Stage 18 rustdoc include all
features. Frontend evidence includes the WASM prebuild, not only Vite.

[environment.json](environment.json) records the actual Dev Container image and
setup. The first existing container lacked Git LFS filters and reported a
hydrated PNG as dirty. Its 24,069 bytes already matched the committed LFS object
SHA-256; no image repair was needed. Installing Git LFS and configuring local
filters resolved that environment discrepancy. The first report's dirty flag
is retained rather than rewritten as a clean run.

[attempts.json](attempts.json) preserves the failed `macos-full-01` and the
successful targeted follow-up. The coordinator omitted the existing
`--arena-initial-expected` argument for the edited initial-package scenario.
Passing the required detached initial oracle allowed the actual Player to verify
all 1,800 ticks and 1,816 inputs, with the same signed native library. This does
not relabel the failed complete attempt. The first build also generated
Addressables `link.xml`/metadata outside its initial cleanup allowlist; retained
copies and the later preservation/restoration fix are recorded separately.

[macos.json](macos.json) records the successful complete `macos-full-02` from
clean `7db01caeaa98e6ed353fc4b2aa9526e4c8704b0b`. It retains the native, Unity,
C caller, standalone, content and cleanup results, including the exact package,
Player and signed library identities. The default and edited Players use the
same signed native library; only the bundled sources select the edited initial
values. The failed first attempt remains distinct in [attempts.json](attempts.json).

[browser.json](browser.json) records both actual Admin flows: complete displayed
state, cue/entity inspection, forward/backward replay, step, Play to EOF, pause,
Stop, return-live, stale/reconnect and 390px layout all passed without page errors.
The embedded flow used the relocated Player, with the external Arena WebSocket
override removed; its executable identity is retained.
[runtime-archive.json](runtime-archive.json) records eight retained artifacts and
two signed Players under the ignored `stage18-c6b392508f27` archive.
Each Player's `manifestSha256` hashes its compact UTF-8 JSON file inventory
before the saved trailing LF; individual payload hashes cover the actual file
bytes. All 686 archived Player files were independently checked against that
inventory.
[cleanup.json](cleanup.json) records the stopped task-owned processes and released
ports. Recordings retain their original IDs and execution metadata.

Build output, full state and trace files, recordings, runtime archives and PNGs
remain local ignored artifacts. Compact reports retain commands, counts, versions
and hashes; this stage publishes no screenshots.

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

No in-scope feature has been deferred as structurally impossible. Implementation
and required local verification are complete. The remaining publication, CI,
review, merge and parent reference update depend on resolving the GitHub push
approval blocker; they are not structural impossibility claims.
