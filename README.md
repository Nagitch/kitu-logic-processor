# Kitu Logic Processor

**Kitu**, a Rust–Unity hybrid framework for building deterministic, data‑driven games with a strong focus on developer experience.

---

## 1. Introduction

Kitu separates **authoritative game logic** (Rust backend) from **presentation** (Unity). The backend runs the simulation, ECS, timelines, and scripting; Unity renders visuals, audio, and UI, and forwards player input as OSC events.

Goals:

- Deterministic, server‑authoritative simulation (even for single‑player)
- Unity as a pure presentation client
- OSC/osc‑ir as a common event protocol across all tools
- Fast iteration via hot‑reloadable data and scripts
- Strong tooling (Shell, Web Admin, CI/replay)
- Minimal coupling between teams (logic / art / design / QA)

---

## 2. High‑Level Architecture

At a high level, Kitu consists of:

- **Rust backend**
  - ECS‑based simulation (Bevy ECS or equivalent)
  - Rhai scripting for game logic DSL
  - TSQ1 timeline playback
  - TMD + SQLite for master data
  - High‑precision game loop and deterministic execution
- **Unity client**
  - Receives OSC events and renders world/UI/audio
  - Sends input as OSC events to backend
  - Loads assets via Addressables
  - Contains no gameplay rules
- **Communication layer**
  - osc‑ir data model (OSC‑compatible IR)
  - Native structs (embedded mode) or MessagePack (network mode)
- **Tooling**
  - Kitu Shell (CLI console)
  - Web Admin (browser UI)
  - CI/replay, automation scripts

The guiding principles are: separation of concerns, determinism, data‑driven design, and event‑based communication.

---

## 3. Backend Game Logic (Rust)

The Rust backend is the authoritative “game universe”.

Core responsibilities:

- ECS entities/components/systems for all simulation
- Game loop driven by a high‑precision timer
- Handling OSC input events from Unity/Web Admin/tests
- Running Rhai scripts for configurable behavior (skills, quests, AI, etc.)
- Executing TSQ1 timelines
- Loading and validating TMD + SQLite data
- Producing OSC output events for Unity and tools

The backend can run:

- As a **standalone server binary** (for dev/multiplayer/CI)
- As an **embedded cdylib** inside Unity (for offline or single‑player builds)

In both cases, the logic code is identical, ensuring identical behavior.

---

## 4. Communication Layer (OSC + osc‑ir + MessagePack)

Kitu uses OSC semantics and the **osc‑ir** data model as a unified event layer.

- OSC provides hierarchical addresses like `/input/move`, `/game/spawn`, `/ui/dialog`.
- osc‑ir defines a strongly typed, serializable representation of OSC messages.
- In embedded mode, events can be passed as native structs or byte buffers.
- In networked mode, events are serialized with MessagePack and sent over WebSocket/TCP.

Typical flows:

- Unity → Backend: `/input/*`, debug commands, UI actions
- Backend → Unity: `/render/*`, `/ui/*`, timeline triggers
- Web Admin ↔ Backend: `/debug/*`, inspection and control events

This event‑driven layer allows loose coupling and shared tooling.

---

## 5. Unity Client (Presentation Layer)

Unity is treated purely as a **renderer and input source**:

- No game rules or state machines live in MonoBehaviours.
- All GameObject creation/destruction is driven by backend `/render/*` events.
- Animations, camera moves, VFX, and UI changes are reactions to backend output.
- Input is converted into OSC events and sent to the backend.

Unity responsibilities:

- Visuals (3D/2D, shaders, VFX, lighting)
- Audio playback
- UI rendering (HUD, menus, dialogs)
- Scene/screen transitions
- Asset loading via Addressables (using stable keys provided by backend/data)

This keeps Unity relatively simple and reduces coupling.

---

## 6. Kitu Shell

Kitu Shell is a runtime developer console that can connect to:

- Local standalone backend
- Remote backend over the network
- Unity‑embedded backend (via a bridge)
- The browser‑based shell inside Web Admin

It allows:

- Listing and inspecting entities and components
- Controlling TSQ1 timelines (play/seek/stop)
- Adjusting data at runtime (dev‑only)
- Sending arbitrary OSC events (`send /game/spawn_enemy { ... }`)
- Running automated scenario scripts
- Performing deterministic replays

Shell commands use a consistent, extensible command model, and all actions go through the same event pipeline as normal gameplay.

---

## 7. Data Systems (TMD + SQLite)

Kitu is strongly data‑driven:

- **Tanu Markdown (TMD)** is used for human‑editable master data:
  - Units, items, skills, quests, config tables, etc.
  - Git‑friendly, readable, and diffable.
  - Supports layering/overrides (base, difficulty, event, debug).
- **SQLite** is used for larger or more relational datasets:
  - Complex catalogs, localization tables, graphs, analytics, etc.

The backend:

- Loads and validates all TMD/SQLite data at startup or on hot reload.
- Reports validation errors through logs, Shell, and Web Admin.
- Provides typed accessors to systems and scripts.
- Supports hot reload of data in development workflows.

Unity typically does not read TMD/DB directly; it acts on backend results.

---

## 8. Timeline & Automation (TSQ1)

TSQ1 is Kitu’s **minimal, deterministic timeline format**:

- Stores time‑ordered events (with tracks, markers, metadata).
- Used for cutscenes, UI transitions, scripted sequences, automation.
- Executed entirely in the backend; Unity just consumes resulting events.

Important: **TSQ1 does *not* define tween/easing itself**.  
Interpolation and easing are implemented at the application layer:

- Camera or UI interpolators interpret events like “move_to with duration”.
- Backend/Unity code applies linear/eased curves, blending, crossfades.
- TSQ1 remains simple, deterministic, and domain‑agnostic.

TSQ1 is authored via TMD, direct files, scripts, or (in future) visual editors.  
It is also a powerful tool for scenario testing and automation.

---

## 9. Web Admin Tools

Web Admin is a browser‑based control plane for Kitu:

- Runs against any backend instance (local or remote).
- Provides dashboards, ECS inspectors, timeline views, minimaps, logs, and metrics.
- Embeds a browser version of Kitu Shell.
- Allows safe, authenticated debugging and live‑ops control.

Key capabilities:

- Inspect entities/components and timelines in real time.
- Monitor event streams and logs via WebSocket.
- Run Rhai scripts or Shell commands from the browser.
- Visualize world state (logical minimap) for debugging AI and level design.
- Enforce permissions and roles (viewer, dev, designer, admin, live‑ops).

Web Admin turns the backend into an observable, controllable system without needing Unity running.

---

## 10. Development Workflow

Kitu’s workflow focuses on **fast iteration and strong separation**:

- Backend developers work in Rust and Rhai, testing logic headlessly.
- Unity developers focus on visuals, UI, asset setup, and Addressables.
- Designers edit TMD, TSQ1, and DB content, often without touching code.
- QA and automation teams use Shell/Web Admin and deterministic replays.

Typical loop:

1. Run backend (standalone or embedded in Unity).
2. Start Unity for visual feedback (if needed).
3. Edit data (TMD/TSQ1/DB) or scripts (Rhai).
4. Hot reload in backend; Unity stays running.
5. Inspect and tweak using Shell/Web Admin.
6. Commit once behavior and visuals are satisfactory.

CI can run backend‑only scenario tests and performance checks without Unity.

---

## 11. Deployment & Distribution

Kitu supports multiple deployment patterns:

- **Single‑player / offline**:
  - Backend as cdylib embedded in Unity.
  - Local SQLite DB and TMD shipped with client.
- **Client–server multiplayer**:
  - Backend as standalone binary (Docker/Kubernetes/VM).
  - Unity clients connect via WebSocket/MessagePack.
  - Web Admin exposed (with auth) for operations and debugging.
- **Hybrid**:
  - Offline capability with optional online features.
  - Backend may run both embedded and remotely, sharing event protocol.

Additional aspects:

- Addressables hosted on CDNs; versioning by Git hash or build IDs.
- Data (TMD/TSQ1) can be shipped with client or delivered server‑side.
- CI/CD pipelines build backend, client, bundles, and data, and deploy them.
- Version compatibility checks between client and backend.

---

## 12. Roadmap (TBD)

The long‑term roadmap is still being defined. Areas under consideration include:

- Richer Web Admin visualization and editors (TSQ1, ECS, AI, quests).
- Unity editor tooling and VSCode extensions for Kitu formats.
- Advanced multiplayer patterns and sharding.
- AI‑assisted balancing and automated playtesting.
- Ready‑made templates for common game genres.

A detailed, versioned roadmap will be published once the core architecture and workflows have stabilized through real projects.

---

Kitu is intended as a long‑term foundation for building modern, data‑driven games with Rust and Unity. This README provides a high‑level architectural overview; individual crates, packages, and tools should provide more detailed API‑level documentation.
