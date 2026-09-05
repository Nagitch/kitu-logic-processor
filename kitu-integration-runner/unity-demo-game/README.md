# Unity Demo Game Verification App

This directory is reserved for the minimal Unity client project used by CI/CD and integration tests.

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
- Hosting full game-specific implementation.

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

Restart the terminal after installation, then run from this directory:

```sh
cd kitu-unity-demo-game
unity --version
unity open .
```

Install Editor `6000.6.0f1` through Unity Hub if it is missing, and activate a
Unity license on the machine. Let the initial package import and script
compilation finish. From a second terminal in the same project directory:

```sh
unity command editor_status --json
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

## Unity-only action RPG demo

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
