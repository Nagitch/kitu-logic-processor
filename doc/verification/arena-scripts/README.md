# Arena Rhai verification

Stage 13, Issue #154. The [boss contract](../../specs/arena-boss-scripts.md) describes
source versions, execution bounds, cache-only admission and next-run activation.

Verified implementation evidence:

- Dev Container: 274 tests/doctests in 52 suites, zero failed/ignored; formatting,
  all-target/all-feature Clippy and warnings-denied rustdoc passed. [Counts](rust.json).
- Real Rhai: 11 shared-host tests and a doctest; standalone and Tanu's existing
  feature combination pass. No Tanu global feature changes were needed.
- macOS: 14 native/application/HTTP/WS/lifetime tests/doctests passed. The first new
  fixture test exposed a test-only relative path error; its corrected test and
  the remaining bridge/doc suites passed. The existing five native tests had
  already passed, including the full frozen stock comparison.
- The actual C caller verifies complete state and output for 28 preparation ticks,
  5528 stock ticks and 1800 edited-script ticks, including ten lifetime cycles per
  trace. [Exact comparisons and hashes](native-c.json).
- The edited script stages telegraph duration 1.6 seconds through ordinary input
  admission and produces exactly 96 telegraph ticks. The reference script still
  matches the frozen C# game state, outcomes and tick ordering.
- [Native packaging](native-package.json) and the [macOS Player build](player-build.json)
  succeeded with Unity 6000.6.0f1 and Apple Silicon. All three graphical Player
  scenarios matched every state/output: [preparation](player-preparation.json),
  [stock/death/retry](player-stock.json), and [edited boss](player-script.json).
- Unity [EditMode 50 and PlayMode 21](unity-tests.json) passed with no failures or
  skipped cases. Frontend check, lint and full build including WASM passed.
  [Frontend report](frontend.json).
- The actual [Admin browser flow](browser.json) verifies source inspection,
  valid/invalid edits, next-run activation, and saved-source replay/step/seek.
  The [CLI proof](cli.json) checks 25 commands: 22 successes and three expected
  refusals (stale candidate, syntax error, and valid candidate during replay).
  After deleting the authoring file, the old record reproduced 19,236 ticks,
  four inputs and two runs; final seek and inspected state exactly matched its
  verified state. The live session was returned paused.
- Nine Player and four browser PNGs were inspected locally. [Visual observations
  and image hashes](visual-qa.json) are committed; these new images are not
  uploaded. The stock and edited boss screenshots depict the same onset tick;
  the changed duration is proven by the complete 48-versus-96-tick traces.
- [Aggregate results and retained execution artifacts](results.json) preserve
  the exact tested runtime for these records.

The custom `rhai-boss` trace contains an ordinary script-stage input followed by
the first 1800 frozen stock inputs. It has its own expected output and never
replaces the Unity-only oracle. `ArenaNativeSelfTest` compares every state/output
bundle, checks the 96-tick telegraph duration and captures its rendered boss.

The complete feature-flow reports above identify implementation `54befd3` and its
retained binaries. PR review then identified a shared native/operator ID space:
a native maximum ID could prevent later Admin staging. The [review correction](review-fix.json)
separates protected Admin/Shell script and content producers while preserving
original caller IDs, deduplication and replay. The regression fails on the original
code and passes after the change, including eight mixed-producer replay inputs.
Focused Dev Container verification passed seven native and 28 host cases; a second
read-only review found no additional issue. Final macOS verification passed all
16 native tests/doctests (the new test's
initial expected JSON widened f32; corrected typed serialization passed). Rebuilt
[C comparisons](review-native-c.json), [native package](review-native-package.json),
[Player build](review-player-build.json), [preparation](review-player-preparation.json),
[stock/death/retry](review-player-stock.json), and [edited boss](review-player-script.json)
again match complete state/output for 28/5528/1800 ticks. Existing browser and
Unity test code did not change. The rebuilt Player's [nine local images](review-visual-qa.json)
were inspected again; exact traces confirm both telegraph durations.

Reproduce generic checks inside the Dev Container:

```sh
cargo fmt --all --check
cargo test --all --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
cargo package --list --allow-dirty -p kitu-scripting-rhai
```

Follow the existing [macOS build procedure](../../specs/arena-embedded-host.md)
with `KITU_NATIVE_EVIDENCE_DIR` set to retain all three traces. Run the actual C
caller from [the ABI instructions](../../specs/arena-native-abi.md), then
`tools/run-arena-player-verification.py` for preparation, stock-eleven-death-retry
and rhai-boss. No public package release or screenshot upload is required.
