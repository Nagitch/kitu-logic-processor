# Kitu Demo App assets

Open `EndlessArena/EndlessArena.unity` and enter Play Mode for the Unity-only
endless arena. The opening menu, settings, floor progression, equipment,
medkits, grenades, shields and results run without a Kitu runtime or backend.
See the [project README](../../../README.md) for rules, tests and builds.

`KituDemoAppMain.unity` remains the original XP/potion/objective smoke baseline.

## Endless arena controls

- WASD: move; mouse: aim.
- Left/right mouse: weapon A/B (hold to repeat).
- Z/X: item A/B (single press). Shields defend automatically.
- E: nearby chest. Tab: safe-floor inventory.
- Esc: pause/back. R: retry after death.

## Original smoke controls

- Move: WASD, arrow keys, or gamepad left stick
- Attack: Space, J, left mouse button, or gamepad south/A
- Use potion: E or gamepad west/X
- Restart after victory or defeat: R

## Export and import

Export the `Assets/KituDemoApp` directory from Unity to keep the scene, runtime
scripts, materials, and their metadata together. Import the resulting package
into a Unity 6000.6 project that provides:

- Input System 1.20.0
- Universal Render Pipeline 17.6.0

Assets-window Unity packages do not include Package Manager dependencies or
project settings.
