# Arena packaged content and Addressables

Stage 16, Issue #160. The macOS Arena Player carries its native Runtime, editable
source defaults and local Unity assets. The same native library can initialize
different validated packages without recompiling game logic. Unity owns asset
loading and presentation; Tanu evaluation, boss rules and TSQ1 presentation
timing use the application's existing public APIs and Runtime clock.

## Source package

The source directory is `apps/demo-game/content/`. The packager stages these five
files under Unity's `Assets/StreamingAssets/KituArena/`, alongside a generated
`package.json`. The Player retains that directory under
`Contents/Resources/Data/StreamingAssets/`.

| Ordered path | Maximum bytes | Purpose |
| --- | ---: | --- |
| `unity-assets.json` | 8,192 | Visual roles, Addressable keys and types |
| `arena.tmd` | 131,072 | Initial evaluated game configuration |
| `boss.rhai` | 65,536 | Initial boss script |
| `timelines/boss-telegraph.tsq` | 8,192 | Boss warning assignments |
| `timelines/floor-transition.tsq` | 8,192 | Floor transition assignments |

`package.json` is strict UTF-8 JSON without a BOM, at most 16,384 bytes. Its only
fields are `schemaVersion: 1` and `files`, containing exactly the five entries in
the order above. Each entry contains only `path`, `bytes` and `sha256`: the fixed
relative path, a positive unsigned integer byte count, and 64 lowercase
hexadecimal SHA-256 characters. Empty, oversized, missing, duplicated, unknown
or mismatched entries fail. Unknown or duplicate JSON properties and unsupported
schema versions also fail.

The package identity is SHA-256 of the **exact manifest bytes**. The packager
emits compact JSON with a final LF deterministically; other legal whitespace is
accepted but changes identity. Files must match their recorded lengths and
digests. Unrelated files are ignored. The directory must be absolute and real;
source symlinks, symlinked `timelines`, and nonregular files are refused.
Bounded readers reject devices and FIFOs without waiting for their contents.

The Rust loader evaluates the same captured bytes that it hashes, through
`ContentVersion::from_tmd`, `ScriptVersion::from_source` and
`TimelineVersion::from_sources`; it never reopens a checked source for evaluation.
`kitu_demo_game::arena::package::load_package` returns those versions, the visual
mapping and `PackageIdentity`. That identity's JSON fields are `schemaVersion`,
`hash`, `files`, `contentHash`, `scriptHash` and `timelineHash`.

## Visual mapping and lifetime

Addressables is pinned to **2.11.2** in the Unity manifest and Editor-generated
package lock. `unity-assets.json` contains exactly this ordered role/type shape:

```json
{"schemaVersion":1,"assets":[{"role":"baseMaterial","key":"arena/material/base","type":"Material"},{"role":"cube","key":"arena/primitive/cube","type":"GameObject"},{"role":"capsule","key":"arena/primitive/capsule","type":"GameObject"},{"role":"sphere","key":"arena/primitive/sphere","type":"GameObject"}]}
```

Keys are configurable, unique, nonempty strings of at most 128 UTF-8 bytes,
without NUL. Roles, order and declared types are fixed. The Editor builder
registers the four default addresses independently of the mapping and rejects
requests that its catalog cannot satisfy. An alternate mapping must name
available assets of the declared types; requesting a key does not create it.

`KituArenaContentBuilder.Prepare` creates visual-only primitive prefabs and a
separate copy of the reference material, retaining existing source GUIDs. The
original `Resources/ArenaBase.mat` and procedural Unity-only reference remain
available for comparison. The `Arena Local` group uses the packed builder, LZ4,
local build/load paths, a binary catalog and no remote catalog. The Player build
invokes content generation once, temporarily disables implicit Addressables
generation, and restores the previous Editor setting in `finally`.

`KituArenaClient` validates the package and asynchronously loads one typed
Addressables handle per role before connecting. The view clones retained
prefabs; individual game entities do not start asset loads. A missing bundle,
missing key, wrong type or invalid package produces a preparation diagnostic.
That failed startup creates neither a native Runtime nor a procedural fallback.
Disabling the client destroys its view and releases the handles after Unity
finishes destroying the clones and derived materials. Cancellation during
loading also releases handles. Visual prefabs contain no physics colliders or
game scripts, and loading never advances game time.

## Native initialization

ABI 1 accepts optional `bundledContentDirectory`, an absolute package directory.
It cannot accompany non-null detached `content`, `script` or `timeline`
initializers. The package's three evaluated versions are installed before tick
0. An explicitly invalid package fails creation without compiled-default
fallback. Omitting the option preserves the existing default/detached behavior.

Optional `expectedBundledContentHash` binds creation to a previously checked
manifest. It requires `bundledContentDirectory` and exactly 64 lowercase
hexadecimal characters. Unity supplies it automatically: after asynchronous
visual loading, native captures and validates the package again and compares
identities **before installing a Runtime or creating storage**. A complete,
internally valid package replacement during loading therefore fails instead of
combining one package's visuals with another package's rules. Earlier package
callers may omit this optional check.

`kitu_application_inspect_host_json` includes `package` in `/host/arena/status`:
the captured `PackageIdentity`, or `null`. Paths and host metadata do not enter
deterministic game output. Hashes identify content integrity and versions; they
are not publisher signatures.

## Launch and edit

Ordinary startup selects the shipped package automatically. Development
launches can pass `--arena-package /absolute/package-directory` to select another
complete source package. It supplies initial native versions and visual keys;
Addressables still resolves those keys from the Player's built catalog.

`--arena-storage /absolute/writable-directory` selects authoring and recording
storage. The default is `arena/` under Unity's `Application.persistentDataPath`.
Absent `arena.tmd`, `boss.rhai` and the two `timelines/*.tsq` files receive the
captured package bytes. No-overwrite creation preserves existing edits across
launches and package changes. Explicit external authoring paths are not seeded
or overwritten. The source package itself is not an authoring destination.

Use **Reload and validate** in Admin's Game Parameters, Game Scripts or Story
Sequencing page, inspect the candidate, then **Apply to next run**. A seeded file
never silently replaces initial or active versions. Invalid edits retain the
last valid active/pending versions; the next successful start/retry adopts a
staged version. Existing `--arena-content`, `--arena-script` and
`--arena-timeline` options select external authoring sources with the same rules.
See [layered data](arena-content-sources.md), [boss scripts](arena-boss-scripts.md)
and [presentation timelines](arena-presentation-timelines.md).

## Reproduce preparation and builds

Run commands from the Kitu repository root. General Rust/Python checks use the
Dev Container. Native and Unity builds require an Apple Silicon Mac, Rust
1.96.0 with `aarch64-apple-darwin`, Apple Command Line Tools and the licensed
Unity 6000.6.0f1 Editor with macOS support. Close this checkout's Editor.

```sh
python3 tools/build-arena-native-macos.py --evidence .tmp/stage16/native
python3 tools/build-arena-player-macos.py --evidence .tmp/stage16/player-build
```

The Player tool stages sources, resolves the pinned packages on Editor import,
prepares assets, builds local Addressables content and builds the ARM64 Player.
It checks the actual `.app` package, catalog, settings and bundle bytes against
the content report, plus architecture, native code identity and signatures.
`--content-source /absolute/source-directory` selects different five-file inputs
without rebuilding the native library. `--player` and `--editor` select explicit
output and Editor paths.

For Editor Play Mode without a Player build, first run:

```sh
python3 tools/build-arena-native-macos.py --evidence .tmp/stage16/native
python3 tools/package-arena-content.py --evidence .tmp/stage16/package.json
```

Open the project, finish package import, choose **Kitu → Prepare Arena
Addressables**, then open `KituEndlessArena.unity`. Equivalent batch preparation,
with this checkout's Editor closed:

```sh
ARENA_UNITY_EDITOR="/Applications/Unity/Hub/Editor/6000.6.0f1/Unity.app/Contents/MacOS/Unity"
ARENA_UNITY_PROJECT="$PWD/kitu-integration-runner/unity-demo-game/kitu-unity-demo-game"
"$ARENA_UNITY_EDITOR" -batchmode -quit -projectPath "$ARENA_UNITY_PROJECT" \
  -buildTarget osxuniversal -executeMethod UnityOnlyArena.Editor.KituArenaContentBuilder.Prepare \
  -logFile "$PWD/.tmp/stage16/addressables-prepare.log"
```

To build only packed content, use `KituArenaContentBuilder.BuildLocal` as the
execute method and set `KITU_ARENA_CONTENT_BUILD_REPORT` to an absolute JSON
output path. `package-arena-content.py --inspect` checks the staged package
without rewriting it. Source assets, metadata and Addressables settings are
tracked. Staged packages, native binaries, catalogs, bundles and content-state
outputs are ignored and reproduced by these tools.

## Replay and validation scope

Recordings retain detached content, script and timeline versions, including
initial versions. Replay uses those saved values and bytes after package or
authoring files change or disappear. Existing execution/contract compatibility
checks still apply: earlier executions require retained executables, and
recording identities are never rewritten. Visual assets come from the current
Player catalog; they are not embedded in the recording.

The focused tests are `cargo test --locked -p kitu-demo-game-native --test package`
and `--test packaged_player`. The latter exercises edited initial values through
the C ABI without staging commands. Package-reader and general Rust checks run
in the Dev Container; Apple target checks run on macOS. A build report establishes
packaging, not gameplay or actual asset loading.

Run the graphical startup/cleanup probe separately after building:

```sh
python3 tools/verify-arena-packaged-player.py \
  --player kitu-integration-runner/unity-demo-game/kitu-unity-demo-game/Builds/KituEndlessArena.app \
  --evidence .tmp/stage16/player-content
```

It verifies loaded keys and local bundle dependencies, matching Unity/native
package identities, tick -1 before gameplay, one native owner and released
handles after disabling the view. `--relocate-to /absolute/unused.app` copies and
launches outside the checkout. `--package` selects a test package;
`--expected-package-hash` and `--expected-native-sha256` bind comparison runs.
`--expect-failure` requires a specific diagnostic from the actual Player and
zero native/view ownership. Use the separate
[frozen gameplay verifier](../../kitu-integration-runner/unity-demo-game/README.md#verify-the-built-player-with-the-frozen-scenarios)
for complete state/output comparisons. Retain reports from both kinds of proof.

This stage targets local macOS ARM64 development Players. CDN distribution,
remote catalogs, publisher signing/notarization and other desktop targets are
outside its validation scope.
