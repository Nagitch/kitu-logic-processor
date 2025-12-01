# Kitu MVP Architecture Documentation

## TOC

- [1. Candidate next steps for deeper specification](#1-candidate-next-steps-for-deeper-specification)
- [2. Kitu library layout (crates / Unity packages)](#2-kitu-library-layout-crates--unity-packages)
  - [2.1 Rust workspace layout](#21-rust-workspace-layout)
  - [2.2 Responsibilities of each crate](#22-responsibilities-of-each-crate)
  - [2.3 Game-app repository layout](#23-game-app-repository-layout)
  - [2.4 Responsibilities of each game-* crate](#24-responsibilities-of-each-game--crate)
- [3. Use case list](#3-use-case-list)
  - [A. Boot / main loop](#a-boot--main-loop)
  - [B. Player control / movement](#b-player-control--movement)
  - [C. Battle / enemies / damage](#c-battle--enemies--damage)
  - [D. Status / items / level](#d-status--items--level)
  - [E. Quests / flags / scenario](#e-quests--flags--scenario)
  - [F. Presentation (TSQ1)](#f-presentation-tsq1)
  - [G. UI / menu](#g-ui--menu)
  - [H. Data-driven / hot reload](#h-data-driven--hot-reload)
  - [I. Debug / tools / replay](#i-debug--tools--replay)
  - [J. Save / load](#j-save--load)
- [4. Detailed flows](#4-detailed-flows)

## 1. Candidate next steps for deeper specification

This section collects items to sort out before turning the Kitu architecture into concrete specs and implementations.

1. **Refine communication protocols (OSC / osc-ir / MessagePack).**
2. **Detailed design for the Rust backend (Kitu Runtime).**
3. **Abstraction for the Unity client (presentation layer).**
4. **Kitu spec for TSQ1 timelines.**
5. **Unified model for data-driven flow (TMD + SQLite).**
6. **API design for Rhai scripts.**
7. **Integration of Shell / Web Admin / replay.**

---

## 2. Kitu library layout (crates / Unity packages)

A summary of how to split the Kitu framework itself based on prior discussions.

### 2.1 Rust workspace layout (kitu repository)

```
kitu/
  Cargo.toml              # workspace
  crates/
    kitu-core/
    kitu-ecs/
    kitu-osc-ir/
    kitu-transport/
    kitu-runtime/
    kitu-scripting-rhai/
    kitu-data-tmd/
    kitu-data-sqlite/
    kitu-tsq1/
    kitu-shell/
    kitu-web-admin-backend/
    kitu-unity-ffi/
  tools/
    kitu-cli/
    kitu-replay-runner/
  unity/
    com.kitu.runtime/
    com.kitu.transport/
    com.kitu.editor/
  specs/
    tsq1/
    tmd/
    osc-ir/
```

### 2.2 Responsibilities of each crate

- **kitu-core**: ID types, error types, time management, common utilities.
- **kitu-ecs**: ECS abstraction layer (thin wrapper over bevy_ecs, etc.).
- **kitu-osc-ir**: Types for OSC-like events (address + args).
- **kitu-transport**: Abstractions for sending/receiving over WebSocket / LocalChannel, etc.
- **kitu-runtime**: Tick-based game loop and input/output event management.
- **kitu-scripting-rhai**: Rhai script integration.
- **kitu-data-tmd**: Parse TMD format into structured data.
- **kitu-data-sqlite**: SQLite management, schema, accessors.
- **kitu-tsq1**: TSQ1 AST and playback engine.
- **kitu-shell**: CLI shell (fire /debug events, etc.).
- **kitu-web-admin-backend**: Backend for the Web Admin (HTTP + WS).
- **kitu-unity-ffi**: C API for embedding the cdylib in Unity.

### 2.3 Game-app-side (stella-rpg) repository layout

```
stella-rpg/
  Cargo.toml
  crates/
    game-core/
    game-ecs-features/
    game-data-schema/
    game-data-build/
    game-logic/
    game-timeline/
    game-scripts/
    game-shell-ext/
    game-webadmin-ext/
  data/
    tmd/
    tsq1/
    scripts/
    localization/
  unity/
    com.stella.game/
    com.stella.game.editor/
```

### 2.4 Responsibilities of each game-* crate

- **game-core**: Entry point for StellaGame embedding KituRuntime.
- **game-ecs-features**: Registers components and systems.
- **game-data-schema**: Definitions for game-specific data types (Unit, Item, Skill, etc.).
- **game-data-build**: Builds the datastore from TMD/SQLite.
- **game-logic**: Game rules such as combat and movement.
- **game-timeline**: Game-specific TSQ1 handling.
- **game-scripts**: Exposes Rhai APIs and integrates game logic.
- **game-shell-ext**: Game-specific shell commands.
- **game-webadmin-ext**: Game-specific views/APIs for the Web Admin.

---

This document lists the use cases for applications built with the Kitu framework (template project and Stella RPG) and shows the flow and participating libraries for each.

## 3. Use case list

(This document is continuously updated with content discussed in chat.)

### A. Boot / main loop

- UC-01: Game boot & scene initialization
- UC-02: Main loop (per-tick simulation & rendering updates)

### B. Player control / movement

- UC-10: Player movement
- UC-11: Camera follow

### C. Battle / enemies / damage

- UC-20: Enemy spawn
- UC-21: Player melee attack
- UC-22: Enemy AI actions
- UC-23: HP decrease & death handling

### D. Status / items / level

- UC-30: Experience & level up
- UC-31: Item pickup
- UC-32: Item usage

### E. Quests / flags / scenario

- UC-40: Quest progression
- UC-41: Scenario flag branching

### F. Presentation (TSQ1)

- UC-51: Skill presentation (short TSQ1)

### G. UI / menu

- UC-60: HUD updates
- UC-61: Pause / menu

### H. Data-driven / hot reload

- UC-70: Apply TMD changes
- UC-72: Apply Rhai script changes

### I. Debug / tools / replay

- UC-80: Run debug command from Shell
- UC-81: Monitor state in Web Admin
- UC-82: Replay (input playback)
- UC-83: Run Kitu Shell commands from Web Admin

### J. Save / load

- UC-90: Save/load data

---

## 4. Detailed flows

Detailed flows for UC-01 / UC-02 moved to `kitu_detailed_flows.md`.

Links or summaries for each UC will live here going forward.
