# Unity Demo Game Verification App

This directory is reserved for the minimal Unity client project used by CI/CD and integration tests.

It pairs with `apps/demo-game` while staying focused on the Unity presentation/input boundary.

## Project

- Unity project path: `kitu-unity-demo-game/`
- Unity version: `6000.5.0f1`
- Version control mode: Visible Meta Files
- Asset serialization mode: Force Text

Purpose:
- Verify that Unity <-> standalone Rust runtime integration remains bootable during development.
- Verify that Unity <-> `kitu-unity-ffi` integration remains bootable for embedded builds.
- Run smoke-level checks that the application boundary is not broken.
- Support regression checks for representative runtime flows.

Non-goal:
- Hosting full game-specific implementation.

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
Unity Input System `1.19.0` and Universal Render Pipeline `17.5.0`, because
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

2. Open `kitu-unity-demo-game/` in Unity `6000.5.0f1`.
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
