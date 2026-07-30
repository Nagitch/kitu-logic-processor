# Kitu Demo App assets

This directory contains the complete asset set for the Unity-only action RPG
demo. Open `KituDemoAppMain.unity` and enter Play Mode; no Kitu runtime or backend
process is required.

## Controls

- Move: WASD, arrow keys, or gamepad left stick
- Attack: Space, J, left mouse button, or gamepad south/A
- Use potion: E or gamepad west/X
- Restart after victory or defeat: R

## Export and import

Export the `Assets/KituDemoApp` directory from Unity to keep the scene, runtime
scripts, materials, and their metadata together. Import the resulting package
into a Unity 6000.5 project that provides:

- Input System 1.19.0
- Universal Render Pipeline 17.5.0

Assets-window Unity packages do not include Package Manager dependencies or
project settings.
