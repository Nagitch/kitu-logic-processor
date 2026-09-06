# Stage 16: packaged content and Addressables

[Issue 160](https://github.com/Nagitch/kitu-logic-processor/issues/160).
The [package contract](../../specs/arena-packaged-content.md) defines source
validation, initial versions, Addressables loading and authoring behavior.
[results.json](results.json) records the current evidence and pending work;
local verification does not imply that the stage has merged.

**Pending:** CI, review, normal merge and the parent workspace update. All local
checks listed below have passed.

| Verification | Result |
| --- | --- |
| Dev Container workspace | 343 tests/doctests in 58 suites; zero failures or ignored tests |
| Final package replacement guard | Five focused native package tests passed after the full workspace run |
| Rust tooling | Formatting, all-target/all-feature Clippy and warnings-denied rustdoc passed |
| Unity 6000.6.0f1 | 143 EditMode, all 29 MessagePack PlayMode and six JSON network PlayMode cases passed |
| Real Addressables 2.11.2 build | Four typed asset keys in one local bundle and binary catalog; Player copies match recorded hashes |
| Default packaged Player | Four scenarios match every complete logical state and output: 9,186 ticks / 9,257 inputs |
| Edited package, unchanged native dylib | 1,800 ticks / 1,816 inputs match; three required rendered checkpoints exist |
| Relocated standalone startup | Default and edited packages load four actual assets, one native owner and one view before tick 0 |
| Existing authoring files | A further launch preserves all four existing files byte-for-byte |
| Invalid Player startup | Six actual graphical Player failures diagnosed; no native owner, gameplay tick or fallback view |
| Local pixel comparison | 15 images differ from Stage 15 by at most 3/255 per channel; exact pixel equality is not claimed |

## Content and ownership proof

[Build reports](builds.json) bind the macOS ARM64 native library, both Player
builds, source packages, catalog and local bundle. The native dylib's SHA-256 is
`0fd76742589ebeb0ee13566dfd32de5359c3b5146071d5b90a2e556c8b2db206`
in both packages. The default manifest hash is
`0e003376f7788866f6484137bedbfe2ef62c7c8b7d9f7ac61994eeb0c59406e5`;
the edited manifest hash is
`ed49d3b0010dfedcfeee278f91a580146a8d267bfcf96c6daef1142515af2646`.
The packed catalog contains `arena/material/base` and the cube, capsule and
sphere keys. Its one 259,644-byte bundle uses a runtime-relative local path.

[Actual bootstrap probes](bootstrap.json) launch relocated `.app` copies outside
the checkout. They record the loaded keys/types and resolved bundle dependency
paths beneath the selected app; neither a remote resource nor an Editor
AssetDatabase fallback satisfies the checks. Unity and native inspection report
the same package identity. Before gameplay, tick remains -1 with one native
handle, four asset leases and one presentation. Disabling the client leaves no
native handles, asset handles or procedural reference game.

The edited package supplies starter damage 32, a 1.6-second Rhai telegraph,
boss warning radius 4.5 and floor opacity 0.85. Its configured initial versions
are checked before the first start. The pre-start inventory is not mistaken
for an already adopted run configuration. The [native fixture](results.json)
observes 96 telegraph ticks, uses zero staging inputs and verifies detached
replay. The final graphical run proves those packaged values through ordinary
inputs and the same native library. This differs from the default Player's
existing Rhai/timeline scenarios, which stage their edits through recorded
management inputs.

First-use authoring copies match the selected package. A separate bootstrap
reuses storage containing an edited TMD and existing Rhai/TSQ files; their
before/after hashes are unchanged while startup still selects the package's
initial versions. File editing retains the documented explicit validate/stage
and next-run adoption rules.

## Gameplay and failure cases

[players.json](players.json) retains each command, source-report digest,
trace/expected/actual hashes, executable/native identities and local screenshot
hashes. The independent verifier compares parsed JSON for every full state and
output batch. Raw NDJSON serialization can differ while those logical values
are equal.

| Scenario | Ticks | Inputs | Observation |
| --- | ---: | ---: | --- |
| Preparation | 28 | 40 | Inventory and return to opening |
| Stock | 5,528 | 5,581 | 11F, natural death and retry |
| Recorded Rhai edit | 1,800 | 1,817 | Longer boss telegraph |
| Recorded timeline edit | 1,830 | 1,819 | Authored warning/floor cues and 30 paused ticks |
| Edited initial package | 1,800 | 1,816 | Packaged TMD, Rhai and TSQ values; no staging inputs |

The first edited graphical attempt reached the final checkpoint check but
required a weapon screenshot using a chest condition. The corrected harness
observes the equipped starter. The final build reproduces the same trace and
complete state/output, with `bundled-weapon.png`, `bundled-boss.png` and
`bundled-floor.png` all present. The native library did not change for this
checkpoint correction.

[failures.json](failures.json) records six separate actual Player processes:
missing bundle, unavailable key, a key resolving to the wrong type, source
digest mismatch, unsupported package schema and invalid packaged Rhai. Each
returns the requested diagnostic with tick -1, zero native handle peak and no
retained view/assets. These results come from the Player, not a preliminary
Python rejection of invalid files.

[visuals.json](visuals.json) compares the 15 default-scenario images with their
Stage 15 counterparts. All have small pixel differences; the largest absolute
channel difference is 3 on the 0–255 scale. This evidence supports the measured
rendering comparison without relaxing exact gameplay/state/event comparison.
PNG files remain local. Only image paths, hashes and measurements are included;
no screenshots were uploaded for this stage.

## Reproduce and interpret the scope

Use the [preparation/build instructions](../../specs/arena-packaged-content.md#reproduce-preparation-and-builds)
for a fresh checkout and the
[Player scenario instructions](../../../kitu-integration-runner/unity-demo-game/README.md#verify-the-built-player-with-the-frozen-scenarios)
for graphical comparison. The reports retain exact invoked paths and commands.
Run general checks in the Dev Container:

```sh
cargo fmt --all -- --check
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps
cargo test --locked -p kitu-demo-game-native --test package
```

[rust.json](rust.json) preserves the actual command scope: the workspace tests
and rustdoc did not add `--all-features`; Clippy did. The full 343-test run
preceded the `expectedBundledContentHash` replacement guard, so the final five
package regressions are listed separately rather than counted as new distinct
workspace tests. Final Clippy compiled that guard. Python package generation
also passed the Rust loader/C ABI interoperation check; 17 helper checks cover
staging, malformed sources and actual packed-file validation.

[unity-tests.json](unity-tests.json) distinguishes the completed EditMode and
full MessagePack PlayMode cases from the six JSON network cases. The seven
scoped Addressables cases also appear in the full 29-case suite. Native/Unity checks use
the macOS SDK; this evidence makes no Windows, CDN, remote catalog or production
distribution claim. Large traces, binaries and images remain ignored local
artifacts, identified by their recorded hashes.
