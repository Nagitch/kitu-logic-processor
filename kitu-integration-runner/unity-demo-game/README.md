# Unity Demo Game

This Unity project contains the Kitu-backed endless arena, its Unity-only
comparison baseline, and the separate Kitu integration verification scene.

It pairs with `apps/demo-game` while staying focused on the Unity presentation/input boundary.

## Project

- Unity project path: `kitu-unity-demo-game/`
- Unity version: `6000.6.0f1` (Unity 6.6)
- Version control mode: Visible Meta Files
- Asset serialization mode: Force Text

Purpose:
- Verify that Unity <-> standalone Rust runtime integration remains bootable during development.
- Verify that Unity <-> `kitu-unity-ffi` integration remains bootable for embedded builds.
- Run smoke-level checks that the application boundary is not broken.
- Support regression checks for representative runtime flows.

Non-goal:
- Production-quality content and presentation.

## Unity CLI

The project includes the official `com.unity.pipeline` package, pinned to
`0.6.0-exp.1`. The commands below use Unity CLI `1.0.0-beta.8`. Both are
prerelease tooling; update them deliberately and verify this workflow when
changing versions. The Editor version is pinned in
`kitu-unity-demo-game/ProjectSettings/ProjectVersion.txt`.

Install the CLI using the [official installation instructions](https://docs.unity.com/en-us/unity-cli/use-unity-cli).
For the verified macOS/Linux CLI version:

```sh
curl -fsSL https://public-cdn.cloud.unity3d.com/hub/prod/cli/install.sh -o /tmp/unity-cli-install.sh
UNITY_CLI_CHANNEL=beta UNITY_CLI_VERSION=1.0.0-beta.8 bash /tmp/unity-cli-install.sh
```

The official installer installs a persistent CLI and configures the interactive
shell's PATH. On macOS the default executable is `~/.unity/bin/unity`. Restart
the terminal after installation, or load `~/.unity/env` in an existing macOS
shell. Non-interactive shells do not necessarily read `.zshrc`.

Then run from this directory:

```sh
cd kitu-unity-demo-game
unity --version
unity open .
```

### Select the intended checkout

Use `scripts/unity-cli.sh` when invoking the CLI from automation or elsewhere
in the repository. It discovers the installed CLI from PATH, the default
macOS location, or the default Linux location, and selects the Unity project
next to the script. From the repository root:

```sh
./kitu-integration-runner/unity-demo-game/scripts/unity-cli.sh --version
./kitu-integration-runner/unity-demo-game/scripts/unity-cli.sh command editor_status --json
```

`KITU_UNITY_CLI` can specify an absolute executable path for a custom CLI
installation. `KITU_UNITY_PROJECT` can select a different checkout explicitly:

```sh
KITU_UNITY_PROJECT=/absolute/path/to/kitu-unity-demo-game \
  ./kitu-integration-runner/unity-demo-game/scripts/unity-cli.sh command editor_status --json
```

Check the returned `projectPath` and `unityVersion` (`6000.6.0f1`) before
performing scene operations. Git worktrees have separate project directories
and Unity caches. The Hub entry for a regular checkout does not automatically
switch to a worktree or receive its commits when a PR is created. Update the
checkout registered in Hub to the merged commit, or explicitly add/open the
worktree you want to use. Re-registering an old checkout in Hub does not update
its files. The helper does not change Hub registration or Git branches.

### Verify the Editor connection

Install Editor `6000.6.0f1` through Unity Hub if it is missing, and activate a
Unity license on the machine. Let the initial package import and script
compilation finish. From a second terminal in the same project directory:

```sh
unity command editor_status --json
unity command eval --code 'return UnityEngine.Application.unityVersion;' --json
unity command list_open_scenes --json
unity command open_scene --path Assets/KituDemoApp/KituDemoAppMain.unity
unity command editor_play
unity command editor_status --json
unity command get_console_logs --severity error --json
unity command editor_stop
```

The play/stop commands initiate transitions; wait until `editor_status` reports
the requested state before the next scene operation. `unity command` lists the
available commands. Pass `--project-path /absolute/path/to/kitu-unity-demo-game`
before the command name when running outside the project directory.
The same commands can be passed to `scripts/unity-cli.sh`. A successful
connection reports the intended project, the expected Editor version, and no
compilation/domain reload in progress. A missing Pipeline instance during the
first import is not a successful connection: wait for compilation to finish
and check again. `get_console_logs --severity error` should return an empty log
list after the demo smoke check.

The Pipeline dependency is already committed, so a fresh checkout does not
need `unity pipeline install`. Its Editor connection metadata lives under the
ignored `Library/Pipeline/` directory. The Unity-only gameplay assets do not
depend on Pipeline, and no Pipeline runtime server is enabled in the demo.

For a repeatable import/compile check, close this project's Editor and run:

```sh
mkdir -p Logs
unity run . --timeout 600 -- -nographics -quit -accept-apiupdate -logFile Logs/import.log
```

If the CLI launcher exits or stalls before the Editor starts, use the Editor
directly. This fallback was used for validation on macOS; `unity command`
still connects to an Editor started this way:

```sh
UNITY_EDITOR="/Applications/Unity/Hub/Editor/6000.6.0f1/Unity.app/Contents/MacOS/Unity"
"$UNITY_EDITOR" -projectPath "$PWD"
```

With that project's Editor closed, the direct batch checks are:

```sh
mkdir -p Logs Builds
"$UNITY_EDITOR" -batchmode -nographics -quit -accept-apiupdate \
  -projectPath "$PWD" -logFile "$PWD/Logs/import.log"
"$UNITY_EDITOR" -batchmode -quit -projectPath "$PWD" \
  -buildTarget osxuniversal -buildOSXUniversalPlayer "$PWD/Builds/Demo.app" \
  -logFile "$PWD/Logs/build-macos.log"
```

Unity 6.6 upgrades the core package set, including URP `17.6.0`, Input System
`1.20.0`, uGUI `2.6.0`, and Test Framework `1.8.0`. Keep the Editor-generated
`Packages/packages-lock.json` and project settings changes together. The
existing reload-domain-and-scene Play Mode setting is retained; changing the
reload policy is a separate behavior change.

References: [Unity 6.6 upgrade guide](https://docs.unity3d.com/6000.6/Documentation/Manual/UpgradeGuideUnity66.html)
and [Unity CLI and Pipeline overview](https://unity.com/blog/meet-the-unity-cli).

## Endless Arena with Kitu (default)

Open `Assets/KituDemoApp/EndlessArena/KituEndlessArena.unity`, the first enabled
build scene. On macOS, inspector `Backend = Automatic` selects the embedded
native library. Build the plugin with the command below before entering Play
Mode. Preparation, inventory/equipment, combat, endless floors, bosses/rewards,
results and retry all run through the same Kitu application as the server.

Set `Backend = Server`, supply `--arena-server ws://127.0.0.1:8787/ws/runtime`,
or set `KITU_ARENA_WS_URL` to select the external server explicitly. Run
`cargo run -p kitu-demo-game --bin kitu-demo-game-admin-host` in the Dev Container
and forward its port for that mode. The inspector `Endpoint` retains its existing
`ws://127.0.0.1:8787/ws/runtime` default.

WASD moves, the mouse aims, mouse buttons fire weapons A/B, Z/X use consumables,
E opens a nearby supply chest, I/Tab opens inventory and Escape closes/pauses.
Choose a backpack destination to take/swap, equip/unequip, discard or upgrade.
Operations use expected item IDs and retain individual shield charge/ownership.
UI changes require release and a fresh press; grenades need a valid ground aim.
Enter the portal after clearing a floor, and collect the optional boss reward
on every fifth floor. Results show the completed run and support retry with R.
Volume/fullscreen settings reuse the original local preferences. Settings opened
from pause leave the run paused after Apply or Cancel.

Kitu owns the 60 Hz game clock, while Unity samples inputs and renders
complete state projections. Disconnect pauses the game; Connect resynchronizes,
and Resume explicitly restarts it with fresh controls. Focus loss requests pause.
Invalid sessions/versions and competing controllers are rejected. The client
never constructs ArenaSimulation. Legacy `/input/move` keeps its original meaning.

Run PlayMode `UnityOnlyArena.Tests.KituArenaConnectionTests` with
`KITU_ARENA_WS_URL` pointing at an isolated running host to include real scene
validation. It checks movement, pause/settings, reconnect, inventory atomicity,
consumable press gating and first-floor combat, then plays a live stock run to
11F, natural death and retry. These external-host tests are explicitly skipped
when that variable is absent; the offline reference suite remains available.
The Rust frozen-input comparison covers all 5,528 stock ticks, 550 complete state
checkpoints and 53 receipts. See [evidence](../../doc/verification/arena-progression/results.json).

**Kitu > Prepare Endless Arena (Kitu)** restores the default build entry without
replacing an existing scene. **Kitu > Build Endless Arena (Kitu, macOS)** builds
the embedded ARM64 scene to `Builds/KituEndlessArena.app` after validating plugin
import settings. The build temporarily selects Mono, ARM64 and background
execution, then restores the Editor's prior build settings.
The original build menu below remains an explicit reference-only choice.

### Reproduce the embedded macOS build

Use an Apple Silicon Mac with Rust `1.96.0`, Cargo, Apple Command Line Tools and
the licensed Unity `6000.6.0f1` Editor with macOS build support. General Rust and
frontend checks remain in the Dev Container; these commands use the Apple SDK.
Close the Editor for this checkout, then run from the repository root:

```sh
python3 tools/build-arena-native-macos.py
python3 tools/build-arena-player-macos.py
```

The first command runs a locked Cargo build for `aarch64-apple-darwin`, installs
`Assets/Plugins/macOS/libkitu_demo_game_native.dylib`, changes its install name
to `@rpath/libkitu_demo_game_native.dylib`, and applies a local ad-hoc signature.
It verifies ARM64, all application ABI exports, system-only dependencies and
the signature before replacing the prior plugin. The binary is ignored by Git;
its stable `.meta` file and Editor importer are tracked. `--profile release`
selects an optimized native build. Cargo/toolchain settings supplied in the
invoking environment are preserved; the default development build disables
incremental compilation and debug information to limit disk use.

The second command selects the pinned Editor, builds the graphical Player and
checks its executable architecture, embedded plugin, signatures and plugin code
identity. `--editor`, `--player`, `--evidence` and `--timeout` override explicit
paths or limits. Build reports and native SHA-256 values are written under
`.tmp/stage11/`; a successful build alone does not claim gameplay verification.

Launch `Builds/KituEndlessArena.app` normally to play without an external Kitu
server. Its development bridge defaults to `http://127.0.0.1:8789` and observes
the same native-owned run. For example:

```sh
cargo run -p kitu-cli -- --endpoint http://127.0.0.1:8789 inspect application
```

Use the Admin frontend against that HTTP endpoint and `ws://127.0.0.1:8789/ws`.
`--arena-bridge off` disables the bridge; `--arena-bridge 127.0.0.1:8790` selects
another loopback port. The bridge queues commands and observes state; it does
not start a second game clock. Inspector fields `NativeBridgeEnabled`,
`NativeBridgeAddress` and `NativeContentPath` configure Editor runs.
Use `--arena-content /absolute/path/to/arena.tmd` to select an existing authoring
document in a standalone run; validate and stage it through Admin for next-run
application.

### Verify the built Player with the frozen scenarios

Generate expected output with the same native target and development profile:

```sh
KITU_NATIVE_EVIDENCE_DIR="$PWD/.tmp/stage11/reference" \
  CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo test --locked --target aarch64-apple-darwin -p kitu-demo-game-native

python3 tools/run-arena-player-verification.py \
  --player kitu-integration-runner/unity-demo-game/kitu-unity-demo-game/Builds/KituEndlessArena.app \
  --trace .tmp/stage11/reference/preparation.trace \
  --expected .tmp/stage11/reference/preparation.expected.ndjson \
  --evidence .tmp/stage11/player-preparation

python3 tools/run-arena-player-verification.py \
  --player kitu-integration-runner/unity-demo-game/kitu-unity-demo-game/Builds/KituEndlessArena.app \
  --trace .tmp/stage11/reference/stock-eleven-death-retry.trace \
  --expected .tmp/stage11/reference/stock-eleven-death-retry.expected.ndjson \
  --evidence .tmp/stage11/player-stock
```

If the plugin was built with `--profile release`, add `--release` to the Cargo
test command as well. Regenerate fixtures whenever the native application,
dependencies or build profile change.

The Player harness forces the native backend, disables its bridge and device
input, and submits the recorded operations through the ordinary native queue
and tick/output path. It compares every complete state and output against the
same-build Runtime expectation, renders checkpoints, writes `result.json`, and
exits with a failing status on divergence. Preparation covers 28 ticks and
40 inputs; the stock scenario covers 5,528 ticks and 5,581 inputs, including 11F
death and retry. The runner independently compares the fresh `actual.ndjson`
against the requested expectation, checks counts, native mode, exit status and
current screenshots, then writes `player-verification.json` with artifact hashes.
Preparation captures `inventory.png`; the stock run captures `chest.png`,
`combat.png`, `boss.png`, `death-11f.png` and `retry.png`.
Extra Player arguments can be passed after `--`; overrides of the three
`--arena-self-test`, `--arena-expected` and `--arena-evidence` flags are rejected.
Screenshots are created by the graphical Player; do not pass `-nographics` when
collecting rendering evidence. A timeout terminates the owned Player process
group, including children that outlive the group leader.

The [Stage 11 verification record](../../doc/verification/arena-embedded/README.md)
retains the actual macOS build reports, both successful standalone comparisons,
six rendered checkpoints and the full Unity results (50 EditMode / 21 PlayMode).
Both graphical standalone fixtures passed with the external server stopped.

## Unity-only endless arena

The [Arena runtime contract](../../doc/specs/arena-runtime-contract.md) defines
the staged Kitu migration and its executable reference traces. The opt-in
`ArenaReferenceSession` records normalized tick commands against the original
C# rules; it does not replace this scene's input or gameplay path. Frozen
preparation and stock-run fixtures live in
`kitu-integration-runner/scenarios/arena/reference/`. Run the
`UnityOnlyArena.Tests.ArenaReferenceTests` EditMode tests to replay them.

Open `kitu-unity-demo-game/` with Unity `6000.6.0f1`, then open
`Assets/KituDemoApp/EndlessArena/EndlessArena.unity` and enter Play Mode.
Choose **Start game** in the opening menu. No Kitu, Rust runtime, network
connection, or backend process is required for gameplay.

The perspective camera follows the player from above and to the side (56-degree
pitch, -72-degree yaw). It keeps the player centered without clamping to arena
edges or zooming out to fit the whole floor. Its initial vertical field of view
is 13.1 degrees at a distance of 55; these values can be tuned on ArenaWorldView.
WASD follows the camera's ground-plane directions, while mouse aiming projects
onto the floor. World labels are clipped to the gameplay viewport.

The opening menu and pause menu offer **Settings** for master volume and
windowed/borderless display. Changes are saved only with **Apply and return**.
Display mode and Quit affect the standalone player, not the Unity Editor.
Sound assets are intentionally absent; the saved master volume is applied to
the shared AudioListener for future audio.

- Move with `WASD`; aim at the floor with the mouse.
- Hold left/right mouse for weapon A/B. Both weapons may fire together.
- Press `Z`/`X` to use item A/B. Medkits restore all HP; grenades explode on
  landing. Both are consumed. Equipped shields absorb damage and recharge
  automatically; they do not need an activation key.
- Walk near the supply chest and press `E`. Select a backpack slot, then
  take/swap a chest item. Equip from the selected slot or use an upgrade.
- `Tab` opens inventory on safe floors. Equipment changes are unavailable
  during combat. Inventory, settings and pause freeze all gameplay clocks.
- Walk into the green portal to advance. Every enemy must be defeated before
  the next portal appears; every fifth floor has a boss and reward chest.
- `Esc` pauses. Death shows results; `R` or **Try again** starts a fresh run.

The 0F chest includes six weapon candidates, a medkit, grenade, shield, and
two upgrades. Three backpack slots and four equipment slots require choices.
There are no enemy item drops, XP, victory endpoint, or run saves. Normal
floors preserve HP; boss clears restore HP once. Shields keep their own charge
between floors, and settings persist independently of runs.

[Game specification](../../doc/specs/unity-only-arena-game.md) and
[verification evidence](../../doc/specs/unity-only-arena-verification.md)
describe the rules and validation scope. Placeholder geometry is generated
by `EndlessArena/Runtime/ArenaWorldView.cs`; meshes and colors can be replaced
without changing the simulation or inventory rules. The original smoke
scene below remains available as comparison evidence.

[Implementation pain log](../../doc/unity-only-arena-pain-log.md) records
observed work around damage/death ordering, item and shield state lifetimes,
and Input System test timing. Kitu improvements are hypotheses to compare
against this working Unity-only baseline.

### Tests and standalone build

From `kitu-unity-demo-game/`, with this checkout's Editor closed:

```sh
arena_editor='/Applications/Unity/Hub/Editor/6000.6.0f1/Unity.app/Contents/MacOS/Unity'
mkdir -p Logs
"$arena_editor" -batchmode -nographics -projectPath "$PWD" \
  -runTests -testPlatform EditMode -testFilter UnityOnlyArena.Tests \
  -testResults Logs/arena-editmode.xml -logFile Logs/arena-editmode.log
"$arena_editor" -batchmode -projectPath "$PWD" \
  -runTests -testPlatform PlayMode -testFilter UnityOnlyArena.Tests \
  -testResults Logs/arena-playmode.xml -logFile Logs/arena-playmode.log
"$arena_editor" -batchmode -quit -projectPath "$PWD" \
  -executeMethod UnityOnlyArena.Editor.ArenaBuild.BuildMac \
  -logFile Logs/arena-build.log
```

The build writes `Builds/EndlessArena.app` and includes only the endless arena
scene. **Kitu > Prepare Unity-only Endless Arena** can recreate a missing arena
scene and place it first in Build Settings while preserving the original
smoke entry. The Hub's regular checkout is a different path from a Git
worktree; open the checkout containing these files to play this version.

## Original Unity-only action RPG smoke baseline

`Assets/KituDemoApp/KituDemoAppMain.unity` is a self-contained gameplay smoke test.
It does not use Kitu, the Rust runtime, networking, or generated content. Open
the scene and enter Play Mode; no backend process is required.

The placeholder arena exercises the smallest complete action RPG loop:

- third-person movement and camera follow
- melee attack, enemy chase/attack, HP, damage, and death
- enemy XP rewards and level-up stat growth
- potion drops, proximity pickup, inventory count, and healing
- an objective that completes after defeating three enemies and collecting a potion
- HUD feedback plus victory/defeat and restart states

Controls:

- Move: `WASD`, arrow keys, or the gamepad left stick
- Attack: `Space`, `J`, left mouse button, or gamepad south/A
- Use potion: `E` or gamepad west/X
- Restart after victory or defeat: `R`

All scene-specific files are contained under `Assets/KituDemoApp/`. Export that
directory as a Unity package to transfer the scene, scripts, materials, and
their GUID-preserving metadata together. The receiving project must provide
Unity Input System `1.20.0` and Universal Render Pipeline `17.6.0`, because
Unity packages exported from the Assets window do not include UPM dependencies.

The gameplay code is isolated under
`Assets/KituDemoApp/UnityOnlyActionRpg/Runtime/` so the placeholder meshes,
materials, arena, and level layout can be replaced without introducing a Kitu
dependency. `ActionRpgGameController` owns the game rules,
`ActionRpgPlayerController` and `ActionRpgEnemy` own actor behavior,
`ActionRpgPickup` owns item collection, and `ActionRpgHud`/
`ActionRpgCameraFollow` cover presentation.

## Development network slice

The first Unity integration path uses a standalone Rust backend instead of
loading a native plugin into Unity.

1. Start the demo backend from the repository root:

   ```sh
   cargo run -p kitu-demo-game --bin kitu-demo-game-admin-host
   ```

2. Open `kitu-unity-demo-game/` in Unity `6000.6.0f1`.
3. Use `Kitu > Build Network Runtime Demo Scene` to create `Assets/Scenes/KituNetworkDemo.unity`.
4. Enter Play Mode and move with the horizontal/vertical input axes.

The Unity client connects to `ws://127.0.0.1:8787/ws/runtime`, sends
`/input/move`, and applies `/render/player/transform` responses to the player
view. This is intentionally separate from the later embedded cdylib/FFI slice.

The same runtime WebSocket also receives world `state` snapshots broadcast by
the demo backend. With the network demo scene in Play Mode, Web Admin
spawn/move/reset actions are mirrored into Unity as primitive scene objects.

## Git management

Tracked project state should stay limited to source assets and deterministic
settings:

- `Assets/`
- `Packages/manifest.json`
- `Packages/packages-lock.json`
- `ProjectSettings/`

Unity-generated caches and local state are ignored from the repository:

- `Library/`
- `Temp/`
- `Obj/`
- `Build/` and `Builds/`
- `Logs/`
- `UserSettings/`
- generated IDE files such as `.csproj`, `.sln`, and `.slnx`

Binary game-content formats are routed through Git LFS by this directory's
`.gitattributes`. Run `git lfs install` before adding large textures, models,
audio, video, fonts, native plugins, or Unity packages.

Unity YAML files are marked with the `unityyamlmerge` merge driver in
`.gitattributes`. Developers who want Unity Smart Merge should configure that
driver locally for their Unity installation path.
