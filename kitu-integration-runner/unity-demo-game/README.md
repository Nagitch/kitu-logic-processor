# Unity Demo Game Verification App

This directory is reserved for the minimal Unity client project used by CI/CD and integration tests.

It pairs with `apps/demo-game` while staying focused on the Unity presentation/input boundary.

Purpose:
- Verify that Unity <-> `kitu-unity-ffi` integration remains bootable.
- Run smoke-level checks that the application boundary is not broken.
- Support regression checks for representative runtime flows.

Non-goal:
- Hosting full game-specific implementation.
