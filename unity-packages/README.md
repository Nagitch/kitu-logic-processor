# Unity Packages

This directory will host Unity packages such as `com.kitu.runtime`, `com.kitu.transport`, and `com.kitu.editor` when the
presentation layer is implemented.

Unity applications should consume shared packages from here instead of owning
reusable bridge code directly. The
[reference Unity demo](https://github.com/Nagitch/kitu-unity-demo-game) exercises
that consumer boundary.
