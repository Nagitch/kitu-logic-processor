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
