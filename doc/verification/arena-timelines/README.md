# Arena TSQ1 presentation verification

Stage 14, Issue #156. The [timeline contract](../../specs/arena-presentation-timelines.md)
defines real TSQ1 source editing, bounded typed bundles, gameplay-step scheduling,
next-run versions and detached replay.

- Dev Container: [303 tests/doctests across 54 suites](rust.json), zero failures or
  ignored tests; formatting, all-target/all-feature Clippy and warnings-denied
  rustdoc passed. The real TSQ1 codec and application preserve typed values,
  bundle boundaries and exact integer scheduling. No dependency-repository edits
  were needed.
- macOS: 19 native/application/HTTP/WS/lifetime tests/doctests passed. A test-only
  replay-load scheduling assumption was corrected: the HTTP response acknowledges
  preparation, and an explicit owner tick activates playback before assertions.
- [Actual C ABI comparisons](native-c.json) match complete state and ordered output
  for preparation (28 ticks), stock 11F/death/retry (5528), the edited Rhai boss
  (1800) and the authored timeline with a 30-tick pause (1830). The frozen C#
  reference source and all fixture hashes remain unchanged.
- [Native packaging](native-package.json) and the [macOS Player build](player-build.json)
  passed. The final graphical Player matches every state/output again for all four
  scenarios: [preparation](player-preparation.json), [stock/death/retry](player-stock.json),
  [edited Rhai boss](player-script.json) and [authored timelines](player-timeline.json).
- Unity [55 EditMode and 21 PlayMode cases](unity-tests.json) passed with no skips,
  including segmented delivery, backward seek, reconnection, absent cues, live
  11F/death/retry and exact Admin/Unity state/presentation comparisons.
- [Admin authoring and playback](browser.json) uses the real UI to validate/stage
  binary edits, retain values after invalid/deleted sources, adopt only at the next
  run, load saved sources and inspect exact floor/boss cue positions at seek and
  step. Six browser images remain local; hashes and observations are retained.
- [Actual CLI verification](cli.json) passed 74 assertions across 41 calls: 37
  successes and four expected refusals (stale candidate, malformed binary, deleted
  source and replay read-only staging). Both the saved 13,950-tick authoring record
  and 1,830-tick cue record replayed while their authoring files were absent;
  seek/step state and cue values matched exactly. The live host was returned paused.
- [Frontend validation](frontend.json) includes Svelte check, lint, the complete
  Rust/WASM and Node build, actual navigation and zero page/console errors.

The custom timeline fixture stages radius 4.5 and floor opacity 0.85 through the
normal input queue. It then runs the first 1800 stock inputs and pauses for 30
management ticks at boss offset 24. The paused image captures pause onset; the
complete trace proves that cue state and the simulation step stay frozen for all
30 ticks and resume at offset 25. Rendering never advances the cue clock.

[Fifteen final Player images](visual-qa.json) were inspected locally; image hashes
and observations are committed. The [retained execution artifacts](results.json)
preserve this exact runtime for these recordings after later implementation changes.

Independent review found that split WebSocket messages could publish a state and
presentation from different ticks. Unity now commits a pair only when management
tick and simulation step both match, resets pending pairs on reconnect and accepts
backward seeks. Actual PlayMode verification also exposed `JsonUtility` creating
an empty cue object for JSON `null`; the presentation decoder now preserves null
with Newtonsoft.Json. Regression tests cover these observed failures.

Reproduce generic checks in the Dev Container:

```sh
cargo fmt --all --check
cargo test --all --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
cd tools/kitu-web-admin/frontend
pnpm check && pnpm lint && pnpm build
```

Generate edited binary sources with `arena-timelines` as described in the
[authoring contract](../../specs/arena-presentation-timelines.md). Follow the
[macOS build procedure](../../specs/arena-embedded-host.md), retaining traces with
an absolute `KITU_NATIVE_EVIDENCE_DIR`. Run the C caller and Player verifier for
`preparation`, `stock-eleven-death-retry`, `rhai-boss` and `timeline-cues`.

PR review identified a detailed timeline snapshot captured before the same tick's
simulation/cue advancement. The [review regression and correction](review-fix.json)
reproduce both start and an unpaused stage at a due floor keyframe, then publish
one coherent snapshot after the tick completes. Receipts, recorded source
versions and per-start manifests remain distinct. Independent review found no
further issue in ordering or replay. The rebuilt [C ABI](review-native-c.json),
[native package](review-native-package.json), [Player](review-player-build.json)
and all four graphical scenarios ([preparation](review-player-preparation.json),
[stock](review-player-stock.json), [script](review-player-script.json),
[timeline](review-player-timeline.json)) pass again. All [15 final PNGs](review-visual-equivalence.json)
are byte-identical to their previously inspected counterparts.
Final [305 Rust tests/doctests, 19 macOS native cases and retained artifacts](review-results.json)
passed, together with formatting, Clippy and warnings-denied rustdoc. Unity
[55 EditMode and 21 PlayMode](review-unity-tests.json) passed again using the final
server recording and native library. Browser/CLI source-edit workflows above
retain their exact earlier execution artifacts; their HTTP contracts did not change.
