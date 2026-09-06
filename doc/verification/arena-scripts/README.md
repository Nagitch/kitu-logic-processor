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
  succeeded with Unity 6000.6.0f 1 and Apple Silicon. Graphical Player and current
  CLI/Admin/Unity test evidence is being collected before merge.

The custom `rhai-boss` trace contains an ordinary script-stage input followed by
the first 1800 frozen stock inputs. It has its own expected output and never
replaces the Unity-only oracle. `ArenaNativeSelfTest` compares every state/output
bundle, checks the 96-tick telegraph duration and captures its rendered boss.

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
