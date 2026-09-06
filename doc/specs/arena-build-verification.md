# Arena build and verification recipe

This is the shared reproduction entry point for Stage 18 of
[roadmap #129](https://github.com/Nagitch/kitu-logic-processor/issues/129).
The [delivery matrix](../verification/arena-delivery/README.md) records actual
completion separately from this recipe. Stage 18 final execution and merge are
pending; the earlier stage records remain historical evidence.

Run commands from the `kitu-logic-processor` repository root. A meta-workspace
checkout must initialize its pinned child repositories before development; keep
their commits and the parent pointer update separate. These checks do not edit
sibling repositories or deploy a service.

Fresh checkouts require Git LFS for the Unity project's tracked binary assets.
Install Git LFS in the environment that reads the checkout, then run these
commands inside `kitu-logic-processor` before opening or building Unity:

```sh
git lfs install --local
git lfs pull
```

The Dev Container includes Git LFS; its setup configures repository-local filters
and checks the installed version. CI checkout requests LFS files explicitly.
An existing container must be rebuilt or provisioned with the same prerequisite.
Missing filters can make an already hydrated asset appear modified, while missing
LFS downloads leave pointer text where Unity expects an image. Resolve the LFS
setup and fetch the files before treating either condition as an asset change.

## Repository checks in the Dev Container

Open this repository in its [Dev Container](../../.devcontainer/devcontainer.json).
The pinned inputs are Rust in `rust-toolchain.toml`, Cargo dependencies in
`Cargo.lock`, Node 24, and `pnpm@11.9.0` with the frontend lockfile. The full
frontend build also needs the `wasm32-unknown-unknown` Rust target. Use the
container setup for these tools rather than relying on unrelated host installs.

Choose a new output directory for every attempt:

```sh
python3 tools/verify-repository.py \
  --scope all --evidence "$PWD/.tmp/verification/repository-01"
```

With the Dev Container CLI, the equivalent invocation from the repository root
is:

```sh
devcontainer exec --workspace-folder . \
  python3 tools/verify-repository.py \
  --scope all --evidence .tmp/verification/repository-01
```

Use one of these invocations, with an absent `--evidence` directory. The tool
refuses to overwrite an earlier attempt and writes `verification.json` plus
per-command logs. It records the requested scope, source commit and dirty-file
identities, dependency locks, commands, results and diagnostics. A nonzero result
is a failed/incomplete check; retain that report before trying a fresh directory.

| Scope | Actual check |
| --- | --- |
| `reference` | Frozen C# source/fixture integrity through `verify-arena-reference.py` |
| `fmt` | `cargo fmt --all -- --check`; formatting is not changed |
| `test` | `cargo test --locked --workspace`; tests must execute without failures or ignored cases |
| `clippy` | `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings` |
| `docs` | `RUSTDOCFLAGS='-D warnings' cargo doc --locked --workspace --no-deps --all-features` |
| `data` | Portable Python tool tests, deterministic source package, and actual Rust loader/C ABI consumption of that package |
| `frontend` | Frozen pnpm install, Svelte check, lint, Inspector tests, and `pnpm run build` including WASM generation |
| `all` | All seven scopes above, in order |

Use a single scope for a relevant follow-up, preserving its name in evidence:

```sh
python3 tools/verify-repository.py \
  --scope frontend --evidence "$PWD/.tmp/verification/frontend-02"
```

The [just recipes](../../justfile) delegate to the same entry point:
`just check-all` runs `all`, while `just verify frontend`, `just fmt-check`,
`just lint`, `just test`, `just doc` and `just data` select a scope and create a
fresh evidence path. `just fmt` is the separate command that changes formatting.

`pnpm exec vite build` alone omits the Rust/WASM prebuild and is not the full
frontend gate. The workspace test scope uses default features; Clippy and this
Stage 18 rustdoc scope include all features. Earlier reports that used default
feature rustdoc retain that narrower command. A repeated focused test is not an
additional unique test in a workspace total.

Portable Python tests use temporary source files and synthetic artifact trees
to exercise packaging boundaries. Actual Unity catalog, bundle, native loading
and graphical proof belong to the macOS scope below; synthetic checks do not
replace those executions.

## macOS native and graphical verification

Use Apple Silicon macOS with Apple command-line/SDK tools available through
`xcrun`, the pinned Rust toolchain and Python 3. Keep one explicit
`aarch64-apple-darwin` target and profile throughout tests, C linking and plugin
packaging. The raw dylib and the relocated/signed plugin have distinct identities;
both must be recorded.

The `native` scope does not require Unity:

```sh
python3 tools/verify-arena-macos.py \
  --scope native --evidence "$PWD/.tmp/verification/macos-native-01"
```

The `full` scope additionally requires the licensed Editor version recorded in
[`ProjectVersion.txt`](../../kitu-integration-runner/unity-demo-game/kitu-unity-demo-game/ProjectSettings/ProjectVersion.txt)
and a usable graphical login session. The current project uses Unity
`6000.6.0f1` and local Addressables `2.11.2`. Close competing project Editors
before running:

```sh
python3 tools/verify-arena-macos.py \
  --scope full --evidence "$PWD/.tmp/verification/macos-full-01"
```

Use `--cargo` for a task-local Cargo executable (default `CARGO` or `cargo`) and
`--profile dev|release` for the build profile (default `dev`). Full scope accepts
`--editor` for the pinned Editor executable and `--port` for an available local
test-host port; without `--port` it selects an available loopback port. Output
must be a new directory. The coordinator
owns its test processes; an occupied endpoint or another Editor must be resolved
without stopping unrelated user processes.

Both scopes atomically maintain `verification.json`, with required steps and
`running`, `passed` or `failed` results. Native-only reports mark full steps
`notRequested`; that is not full-gate success. Command logs live under
`commands/`, with component reports retained beneath the same evidence root.

| Scope | Included proof |
| --- | --- |
| `native` | Toolchain preflight, real source package, native application tests and doctest, raw library/plugin build, actual C caller comparison for preparation, stock, Rhai and timeline scenarios |
| `full` | All native steps; default Player build; owned server/CLI and fresh verified stock recording; EditMode, all MessagePack PlayMode and JSON network cases; C# codec readback; shared inspection execution identity; owned server stop; four default graphical scenarios; relocated/invalid/no-overwrite content probes; same-dylib edited-package build, gameplay and relocated startup |

The [case manifest](../../tools/arena-verification-cases.json) names required
native and NUnit tests. A test runner exit code alone is insufficient: required
cases must actually run without failures or unexpected skips. Four default
trace scenarios cover 9,186 ticks; the separate edited initial-package trace
covers 1,800 ticks without staging inputs. These are required coverage, not a
claim that the current attempt has already passed.

Network tests use the versioned `/ws/arena` controller endpoint, with a recording
created and verified by the corresponding execution.

Build output is a local development artifact. The tool checks native exports,
relocatable install name, signature and bundled identities; it does not publish,
notarize or deploy the app. The [native ABI](arena-native-abi.md),
[embedded host](arena-embedded-host.md) and
[package contract](arena-packaged-content.md) document the lower-level tools and
lifecycle obligations.

The full local gate must establish these separate facts:

- Native tests and the actual C caller linked to the verified signed plugin
  reproduce complete logical outputs; the raw and signed library hashes are retained.
- Both JSON and MessagePack Unity connections accept compatible clients and
  display the same authoritative/replayed state, with required tests enabled.
- The standalone runs the stock game through 11F death/retry with its embedded
  backend and no external game server dependency.
- Local Addressables and bundled TMD/Rhai/TSQ1 sources load from the actual app;
  invalid content fails before game ticks, native/view ownership is released,
  and existing authoring files are preserved.
- An edited initial source package changes the observed game through those
  package bytes while using the same native dylib. Staging equivalent values
  later in a run is a different test.

Complete logical comparison is distinct from rendered image comparison. The
frozen migration contract permits initial absolute `1e-4` floating-state error
but never different hits, deaths, transitions or event ticks. Current native and
Player verifiers compare every complete logical state/output. Screenshots prove
rendered checkpoints; they do not imply bit-identical pixels across builds.

## CI and environment boundaries

The [repository CI workflow](../../.github/workflows/rust-ci.yml) uses the shared
Linux scopes for reference, Rust, frontend/WASM
and portable data checks. The macOS native CI job uses the standard `macos-15`
runner and the native coordinator. It does not claim licensed Unity execution.
Full Unity and graphical Player checks remain an explicit local macOS gate with
recorded evidence, rather than an unprovisioned always-on self-hosted PR runner.

SDK, license, graphical-session and package-download availability are environment
requirements. Report a missing requirement and the unexecuted scope explicitly;
do not count it as a passed gate or call the feature structurally impossible.
Do not solicit or store licensing credentials in scripts or reports.

The separate experimental WebTransport gateway remains outside this Arena
delivery gate. Windows, multiplayer, production authorization/operations, CDN
delivery and cloud deployment are also outside the approved scope. Inspector
timings are observations; this recipe adds no wall-clock performance threshold.

## Reading and retaining evidence

Keep compact command/results JSON with tool versions, source/lock identities,
suite counts, native/package/catalog/bundle hashes and recording identities.
Distinguish a dirty build snapshot from a clean committed build. The execution
fingerprint binds replay compatibility; it does not replace the exact binary
hash or Unity source revision. Server and native executions can have different
fingerprints and must verify their own recordings without rewriting IDs.

Retain the exact runtime binaries needed by older recordings under the ignored
`apps/demo-game/.arena/runtime-versions/` archive before overwriting build output.
Use a new variant directory and inventory hashes; never replace an older archive
under the same label. Full traces, large snapshots, binaries and local screenshots
stay outside Git. The [delivery record](../verification/arena-delivery/README.md)
links compact public evidence and distinguishes local passes, CI, review, child
merge and parent workspace reference publication.

For a live Admin check after building, follow the
[selected server/embedded Inspector workflow](../../tools/kitu-web-admin/README.md#inspect-endless-arena):
configure both API and Admin WebSocket endpoints to the same host, compare the
displayed session/run/tick, inspect a cue/entity, step or seek a saved replay,
and check stale/reconnect behavior and browser errors. Compilation alone does
not prove that the browser observes the intended Runtime. Stage 17's
[server/embedded browser evidence](../verification/arena-inspection/browser.json)
records that complete flow; current checks must keep their own attempt identity.
