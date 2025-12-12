# Kitu Detailed Flow Documentation

## Table of Contents
- [TOC (detailed flow map)](#toc-detailed-flow-map)
- [UC-01: Game boot & scene initialization (detailed flow)](#uc-01-game-boot-scene-initialization-detailed-flow)
- [UC-02: Main loop (per-tick simulation & rendering updates)](#uc-02-main-loop-per-tick-simulation-rendering-updates)
- [UC-10: Player movement (detailed flow)](#uc-10-player-movement-detailed-flow)
- [UC-11: Camera follow (detailed flow)](#uc-11-camera-follow-detailed-flow)
- [UC-20: Enemy spawn (detailed flow)](#uc-20-enemy-spawn-detailed-flow)
- [UC-21: Player melee attack (detailed flow)](#uc-21-player-melee-attack-detailed-flow)
- [UC-22: Enemy AI actions (detailed flow)](#uc-22-enemy-ai-actions-detailed-flow)
- [UC-23: HP decrease & death handling (detailed flow)](#uc-23-hp-decrease-death-handling-detailed-flow)
- [UC-30: Experience & level up (detailed flow)](#uc-30-experience-level-up-detailed-flow)
- [UC-31: Item pickup (detailed flow)](#uc-31-item-pickup-detailed-flow)
- [UC-32: Item usage (detailed flow)](#uc-32-item-usage-detailed-flow)
- [UC-40: Quest start / progress / completion (detailed flow)](#uc-40-quest-start-progress-completion-detailed-flow)
- [UC-41: Scenario flag branching (detailed flow)](#uc-41-scenario-flag-branching-detailed-flow)
- [UC-51: Skill presentation (TSQ1) (detailed flow)](#uc-51-skill-presentation-tsq1-detailed-flow)
- [UC-60: HUD update (detailed flow)](#uc-60-hud-update-detailed-flow)
- [UC-61: Pause / menu (detailed flow)](#uc-61-pause-menu-detailed-flow)
- [UC-70: TMD hot reload (detailed flow)](#uc-70-tmd-hot-reload-detailed-flow)
- [UC-72: Rhai script change → hot reload (detailed flow)](#uc-72-rhai-script-change-hot-reload-detailed-flow)
- [UC-80: Debug commands from Kitu Shell (detailed flow)](#uc-80-debug-commands-from-kitu-shell-detailed-flow)
- [UC-81: State monitoring in Web Admin (detailed flow)](#uc-81-state-monitoring-in-web-admin-detailed-flow)
- [UC-82: Replay (input playback) (detailed flow)](#uc-82-replay-input-playback-detailed-flow)
- [UC-83: Running Kitu Shell commands from Web Admin (detailed flow)](#uc-83-running-kitu-shell-commands-from-web-admin-detailed-flow)
- [UC-90: Save / load (detailed flow)](#uc-90-save-load-detailed-flow)

## TOC (detailed flow map)

- [UC-01: Game boot & scene initialization](#uc-01-game-boot--scene-initialization-detailed-flow)
- [UC-02: Main loop (per-tick simulation & rendering updates)](#uc-02-main-loop-per-tick-simulation--rendering-updates)
- [UC-10: Player movement](#uc-10-player-movement-detailed-flow)
- [UC-11: Camera follow](#uc-11-camera-follow-detailed-flow)
- [UC-20: Enemy spawn](#uc-20-enemy-spawn-detailed-flow)
- [UC-21: Player melee attack](#uc-21-player-melee-attack-detailed-flow)
- [UC-22: Enemy AI actions](#uc-22-enemy-ai-actions-detailed-flow)
- [UC-23: HP decrease & death handling](#uc-23-hp-decrease--death-handling-detailed-flow)
- [UC-30: Experience & level up](#uc-30-experience--level-up-detailed-flow)
- [UC-31: Item pickup](#uc-31-item-pickup-detailed-flow)
- [UC-32: Item usage](#uc-32-item-usage-detailed-flow)
- [UC-40: Quest start / progress / completion](#uc-40-quest-start--progress--completion-detailed-flow)
- [UC-41: Scenario flag branching](#uc-41-scenario-flag-branching-detailed-flow)
- [UC-51: Skill presentation (TSQ1)](#uc-51-skill-presentation-tsq1-detailed-flow)
- [UC-60: HUD update](#uc-60-hud-update-detailed-flow)
- [UC-61: Pause / menu](#uc-61-pause--menu-detailed-flow)
- [UC-70: TMD hot reload](#uc-70-tmd-hot-reload-detailed-flow)
- [UC-72: Rhai script change → hot reload](#uc-72-rhai-script-change--hot-reload-detailed-flow)
- [UC-80: Debug commands from Kitu Shell](#uc-80-debug-commands-from-kitu-shell-detailed-flow)
- [UC-81: State monitoring in Web Admin](#uc-81-state-monitoring-in-web-admin-detailed-flow)
- [UC-82: Replay (input playback)](#uc-82-replay-input-playback-detailed-flow)
- [UC-83: Running Kitu Shell commands from Web Admin](#uc-83-running-kitu-shell-commands-from-web-admin-detailed-flow)
- [UC-90: Save / load](#uc-90-save--load-detailed-flow)


This file collects the detailed architectural flows for each use case (UC-01, UC-02, etc.) implemented with Kitu.


## UC-01: Game boot & scene initialization (detailed flow)

### Architectural checkpoints

- Validate the **repository and crate split** between the Kitu framework and game-specific code (stella-rpg).
- Confirm **cdylib / FFI responsibilities** between Unity and Rust (config passing, lifecycle).
- Ensure layers for **data loading / ECS setup / initial event output** stay coherent at startup.

### Prerequisite: code layout

**Kitu repository (framework)**

- `kitu-runtime`, `kitu-ecs`, `kitu-osc-ir`
- `kitu-data-*` (TMD / SQLite)
- `kitu-tsq1`, `kitu-scripting-rhai`
- `kitu-unity-ffi`

**Game repository (stella-rpg)**

- Rust: `game-core`, `game-data-schema`, `game-data-build`, `game-ecs-features`, `game-logic`, `game-scripts`, `game-timeline`
- Unity: `com.kitu.runtime` (shared bridge), `com.stella.game` (game-specific View layer)

Assumes a **cdylib embedded in Unity**.

### Unity editor Play (initialization begins)

Unity loads the scene and `KituRuntimeBridge` (`com.kitu.runtime`) starts in `Awake()` / `Start()`.

Unity flow:

```csharp
void Start() {
    var configJson = BuildStellaConfigJson();
    KituNative.Initialize(configJson);
}
```

Data passed to Rust:

- Data folder path
- Tick rate
- Logging settings

Rust C API invoked:

```text
kitu-unity-ffi::kitu_initialize(config_json: *const c_char)
```

### Rust: inside `kitu_initialize`

Handled by `kitu-unity-ffi`:

1. Decode JSON → `StellaConfig`.
2. Call `StellaGame::new(config)`.
3. Store the created game instance globally.

Key crates: `kitu-unity-ffi`, `game-core` (`StellaGame::new`).

### `StellaGame::new` (game-layer initialization)

```rust
pub fn new(config: StellaConfig) -> Result<Self, KituError> {
    let mut runtime = KituRuntime::new(config.to_kitu_config())?;

    let datastore = game_data_build::load_datastore(&config.data_root)?;

    game_ecs_features::register_components(runtime.world_mut());
    game_ecs_features::register_systems(runtime.scheduler_mut());

    game_logic::attach_to_runtime(&mut runtime, &datastore)?;
    game_scripts::setup_rhai_api(&mut runtime, &datastore)?;
    game_timeline::setup_timelines(&mut runtime)?;

    Ok(Self { runtime })
}
```

Crates involved: Kitu (`kitu-runtime`, `kitu-ecs`, `kitu-data-*`, `kitu-tsq1`, `kitu-scripting-rhai`) and game-side (`game-core`, `game-data-build`, `game-data-schema`, `game-ecs-features`, `game-logic`, `game-scripts`, `game-timeline`).

### Data loading and validation (TMD / SQLite)

`game_data_build::load_datastore`:

- Reads `data/tmd/**/*.tmd` and parses via `kitu-data-tmd`.
- Converts TMD → AST → SQLite or structs.
- Maps to `game-data-schema` types (e.g., `UnitDef`, `ItemDef`).
- Checks referential integrity (duplicate IDs, missing references).

Purpose: ensure data-driven sections are valid.

### Initial ECS world construction

`game-ecs-features` registers component types and systems (movement, combat, AI, quest, UI, etc.), keeping Kitu ECS abstractions separated from game logic.

### Generate the initial scene (backend → Unity)

During the first tick or immediately after init:

- Spawn the player entity.
- Initialize the map.
- Initialize the HUD.

Queue output events such as:

```
/render/world/init
/render/player/spawn
/ui/hud/show
```

Crates: `kitu-runtime` (output queue), `kitu-osc-ir` (`OscEvent`), game logic (`game-logic`).

### Unity receives output events and builds the scene

`KituRuntimeBridge` in `Start()` or the first `Update()`:

1. Call `KituNative.PollEvents()` to fetch Rust output events.
2. Decode to C# `OscEvent`.
3. Publish to `KituEventBus`.
4. `com.stella.game` views handle rendering (create player GameObject, display enemies/objects, show HUD).

Result: Unity scene reaches its initial state.


## UC-02: Main loop (per-tick simulation & rendering updates)

### Architectural checkpoints

- Run a **fixed-timestep KituRuntime** independent of Unity’s frame rate.
- Keep **deterministic phases** (input → ECS → output events) as the foundation for all use cases.
- Integrate Shell / Web Admin / replay on the same main loop without side effects.

### Overview

Each Unity `Update()` sends `deltaTime` to Rust. KituRuntime advances simulation on a fixed tick rate (e.g., 60 Hz). Logic for collisions, movement, AI, combat, and death runs on the backend; `/render/*` and `/ui/*` events are sent to Unity, which only updates the view.

### Unity → Rust: send `deltaTime` and inputs

```csharp
void Update(){
    var dt = Time.deltaTime;
    KituNative.Update(dt);
    SendInputIfAny();
}
```

Unity responsibilities: notify elapsed time, send input events (e.g., `/input/move`, `/input/attack`), avoid game logic. Rust API: `kitu-unity-ffi::kitu_update(handle, delta_seconds: f32)`.

### Rust: `KituRuntime.update(dt)`

```rust
pub fn update(&mut self, dt: f32) {
    self.time.accumulate(dt);

    while self.time.should_step() {
        self.step_one_tick();
    }
}
```

Accumulates deltaTime, runs as many ticks as needed, and keeps the simulation on a fixed timestep.

### ECS scheduling per tick

Phases (order fixed for determinism):

1. **Input processing**: apply `/input/move`, `/input/attack`, etc. to ECS state.
2. **AI / scripts**: enemy behavior and quest logic via Rhai.
3. **Physics / movement**: apply velocity to position, simple collision.
4. **Combat / damage**: hit checks, skill effects, HP updates.
5. **Death handling**: mark dead entities, despawn.
6. **Collect render data**: transforms, UI info, enqueue Unity-bound events.

Crates: `kitu-ecs`, `game-ecs-features`, `game-logic`, `kitu-tsq1` (when skills present), `kitu-runtime` (event management).

### Output events `/render/*` `/ui/*` `/debug/*`

Results such as:

```
/render/player/transform
/render/enemy/transform
/render/enemy/dead
/ui/hud/update
/debug/log
```

Events use OSC-IR (`kitu-osc-ir`) and accumulate in the KituRuntime output queue for polling by Unity.

### Unity polls events → updates view

```csharp
var events = KituNative.PollEvents();
foreach (var ev in events) {
    KituEventBus.Publish(ev);
}
```

`com.stella.game` consumes events to move transforms, play enemy spawn/death animations, and refresh HUD stats. Unity remains a pure view layer.

### Shell / WebAdmin / replay integration (overview)

- **Shell** (`kitu-shell` / `game-shell-ext`): commands like `spawn_enemy goblin` arrive as `/debug/*` events and are handled in the tick pipeline.
- **Web Admin** (`kitu-web-admin-backend` / `game-webadmin-ext`): connects via WebSocket to read ECS state, logs, and debug results.
- **Replay** (`kitu-replay-runner`): replays logged input per tick to reproduce deterministic outcomes.

All share the same `KituRuntime` tick path, preserving consistent behavior.


## UC-10: Player movement (detailed flow)

### Architectural checkpoints

- Keep **input interpretation in Unity** and **movement logic in Rust** for clear separation.
- Reuse a simple pattern `/input/move` → ECS → `/render/player/transform`.
- Ensure the Kitu ECS abstraction can extend to collisions/terrain later.

### Overview

Unity sends movement input (WASD / stick) as `/input/move`. The backend deterministically updates position and returns `/render/player/transform`; Unity only applies the transform.

- Input parsing: Unity.
- Motion calculation/state: Rust (Kitu + game crates).
- Rendering: Unity.

### Unity: capture input and send `/input/move`

Layers: Unity package `com.kitu.runtime`; C# API `KituNative.SendInputMove`.

```csharp
var axis = new Vector2(Input.GetAxis("Horizontal"), Input.GetAxis("Vertical"));
if (axis.sqrMagnitude > 0.0f)
{
    KituNative.SendInputMove(axis.x, axis.y);
}
```

Event:

```
/address: "/input/move"
args: { x: 0.5, y: 1.0 }
```

### Rust: enqueue input

Crates: `kitu-unity-ffi` (C API → `OscEvent`), `kitu-runtime` (input queue), `kitu-osc-ir` (`OscEvent`). Flow: convert to `OscEvent`, call `KituRuntime::enqueue_input`, defer processing until the next tick’s input phase.

### ECS phase 1: update velocity

Crates: `kitu-ecs`, `game-ecs-features` (`InputMove`, `Velocity`), `game-logic` (speed constants).

```rust
fn input_movement_system(world: &mut World) {
    let move_input = world.resource::<InputMoveState>();

    for (_player, mut velocity) in world.query_mut::<(&PlayerTag, &mut Velocity)>() {
        velocity.x = move_input.x * MOVE_SPEED;
        velocity.y = move_input.y * MOVE_SPEED;
    }
}
```

### ECS phase 3: update position

```rust
fn movement_system(world: &mut World) {
    for (_player, mut pos, vel) in world.query_mut::<(&PlayerTag, &mut Position, &Velocity)>() {
        pos.x += vel.x;
        pos.y += vel.y;
    }
}
```

### ECS phase 6: emit `/render/player/transform`

Crates: `kitu-runtime`, `kitu-osc-ir`.

```rust
fn gather_player_render_events(world: &World, out_events: &mut Vec<OscEvent>) {
    for (_player, pos) in world.query::<(&PlayerTag, &Position)>() {
        out_events.push(OscEvent::render_player_transform(1, pos));
    }
}
```

Resulting event contains entity id and position.

### Unity view applies transform

Layers: Unity `com.kitu.runtime` (event bus) and `com.stella.game` (view). Unity subscribes to `/render/player/transform` and updates the GameObject transform accordingly.


## UC-11: Camera follow (detailed flow)

### Architectural checkpoints

- Keep camera logic in the **Unity view layer only**; Rust exposes the player transform.
- Allow different follow behaviors by swapping Unity-side scripts without touching Kitu.

### Flow

1. Backend continues to emit `/render/player/transform`.
2. Unity `CameraFollowView` subscribes and updates camera position/rotation (e.g., smooth damp toward player, offset by height).
3. Optional: Unity can use the same event for minimap or cinematic cameras without backend changes.


## UC-20: Enemy spawn (detailed flow)

### Architectural checkpoints

- Spawn logic resides in Rust; Unity only instantiates visuals based on events.
- Support multiple spawn triggers (scripted waves, area enter, shell commands).

### Flow

1. **Trigger**: game logic decides to spawn (e.g., area enter, quest flag, shell `/debug/spawn`).
2. **Rust** creates an enemy entity, attaches components (kind, stats, AI state, position), and enqueues `/render/enemy/spawn` with id/kind/prefab/position.
3. **Unity** `EnemySpawnerView` listens to `/render/enemy/spawn`, instantiates the prefab, and binds entity id to the view component.


## UC-21: Player melee attack (detailed flow)

### Architectural checkpoints

- Complete attack logic on the Rust side: hit detection, damage, death.
- Represent results as `/render/*` so Unity can swap VFX freely.
- Keep consistency with networking/replay inputs.

### Flow

1. **Unity input**: on `Fire1`, call `KituNative.SendInputAttack()` → `/input/attack`.
2. **ECS input phase**: set an attack request on the player (`ActionState`, `AttackRequest`).
3. **ECS combat phase**: `melee_combat_system` checks range, computes damage, decreases enemy HP, and emits `/render/enemy/hit`.
4. **HP/death**: when HP ≤ 0, mark dead and hand off to UC-23 for `/render/enemy/dead`.
5. **Unity view**: enemy view plays hit animation/effects on matching entity id.


## UC-22: Enemy AI actions (detailed flow)

### Architectural checkpoints

- Implement AI as ECS systems with **data-driven state transitions**.
- Reuse the same movement/combat pipelines as UC-10/21.

### Flow

1. **AI system registration** with `EnemyAiState` in `game-ecs-features` / `game-logic`.
2. **Decision making** each tick: choose behavior (idle/chase/attack) based on player distance, set velocity or enqueue attack intent.
3. **Reuse pipelines**: velocity feeds movement → `/render/enemy/transform`; attack intent flows into UC-21 combat system.


## UC-23: HP decrease & death handling (detailed flow)

### Architectural checkpoints

- Centralize HP/state transitions in Rust to keep determinism.
- Emit clear events for visuals and loot.

### Flow

1. **Damage application** (from UC-21/other sources) updates `Hp` components.
2. **Death check**: when HP ≤ 0, switch to `Dead` state, despawn logic schedules removal.
3. **Events**: enqueue `/render/enemy/dead` for visuals and `/game/enemy/dead` for downstream systems (drops, quests).
4. **Unity**: plays death animation and removes GameObject when complete.


## UC-30: Experience & level up (detailed flow)

### Architectural checkpoints

- Keep progression math in Rust, UI driven only by events.
- Support future tuning via data tables.

### Flow

1. **XP gain**: combat/quest systems call `award_xp(player, amount)`.
2. **Accumulation**: add to `Experience` component; check thresholds from data (TMD/SQLite).
3. **Level up**: increment level, update stats, enqueue `/ui/levelup` and `/ui/hud/update` events.
4. **Unity**: `LevelUpView` shows popup/animation; HUD refreshes via existing bindings.


## UC-31: Item pickup (detailed flow)

### Architectural checkpoints

- Treat drops → inventory as backend-only domain logic.
- Drive the inventory UI purely through `/ui/inventory/update`.

### Flow

1. **Evaluate drop table** on enemy death or chest open using `game-data-schema` + `game-logic` RNG helpers.
2. **Add to inventory**: mutate `Inventory` component and push `/ui/inventory/update`.
3. **Unity**: `InventoryView` decodes item list and rebinds UI; visuals (icons/layout) remain Unity-only.


## UC-32: Item usage (detailed flow)

### Architectural checkpoints

- Standardize the trigger as `/input/item/use` and express effects through data-driven logic.
- Keep HUD/inventory updates event-driven.

### Flow

1. **Unity**: when the player selects an item, send `/input/item/use` with player id and item slot.
2. **ECS input phase**: set a use-item request on the player.
3. **Effect resolution**: `game-logic` looks up the item definition, applies effects (heal, buff, etc.), updates stats/inventory.
4. **Events**: emit `/ui/hud/update`, `/ui/inventory/update`, and effect-specific render events (e.g., `/render/player/buff`).
5. **Unity**: updates UI and plays VFX; no game-state changes occur on the Unity side.


## UC-40: Quest start / progress / completion (detailed flow)

### Architectural checkpoints

- Model quests and flags as backend data/logic; Unity only renders prompts.
- Allow progression to be driven by Rhai scripts or data tables.

### Flow

1. **Quest definitions** loaded from TMD/SQLite into `QuestDef`/`QuestState` components.
2. **Triggers**: combat kills, pickups, or shell commands raise quest events.
3. **Progress evaluation**: Rhai or Rust rules update flags/stages deterministically.
4. **Events**: notify Unity via `/ui/quest/update` or `/ui/quest/log` for display.


## UC-41: Scenario flag branching (detailed flow)

### Architectural checkpoints

- Keep branching logic data-driven; avoid branching in Unity scripts.

### Flow

1. **Flag store**: backend resource tracks scenario flags.
2. **Mutation**: quests/timeline/scripts set or clear flags.
3. **Consumption**: systems query flags to open paths, spawn NPCs, or gate dialogue; emit render/UI events to reflect changes.


## UC-51: Skill presentation (TSQ1) (detailed flow)

### Architectural checkpoints

- TSQ1 timelines are authored data; Rust evaluates and outputs render cues.
- Unity plays VFX/SFX based on events only.

### Flow

1. **Trigger**: combat system starts a skill timeline (TSQ1 AST via `kitu-tsq1`).
2. **Playback**: timeline steps run during ticks, scheduling `/render/skill/*` and `/ui/*` cues.
3. **Unity**: subscribes to those addresses to play animations, particles, camera shakes.


## UC-60: HUD update (detailed flow)

### Architectural checkpoints

- HUD is entirely event-driven; backend controls content, Unity controls layout/animation.

### Flow

1. Backend collects player stats each tick or on change.
2. Emit `/ui/hud/update` with HP/MP/XP/etc.
3. Unity HUD scripts read payload and update bars/text; animations remain Unity-only.


## UC-61: Pause / menu (detailed flow)

### Architectural checkpoints

- Pause affects the Kitu runtime tick rate; menus remain Unity UI.

### Flow

1. **Unity input**: toggle pause menu.
2. **Rust**: receive `/input/pause` and set runtime state to paused (stop stepping ticks or reduce rate).
3. **Events**: send `/ui/menu/show|hide`; gameplay simulation halts while UI still runs in Unity.


## UC-70: TMD hot reload (detailed flow)

### Architectural checkpoints

- Reload data without rebuilding; keep runtime state consistent.

### Flow

1. Watch TMD files (file watcher or manual command).
2. On change, reparse via `kitu-data-tmd`, rebuild data structures/SQLite.
3. Apply deltas to runtime resources (e.g., stat tables) and notify systems.
4. Emit `/debug/log` or `/ui/notice` to confirm reload.


## UC-72: Rhai script change → hot reload (detailed flow)

### Architectural checkpoints

- Swap scripts safely without restarting the runtime.

### Flow

1. Detect script file change.
2. Recompile Rhai modules and update function bindings.
3. Refresh any cached script state in systems.
4. Emit a debug/UI event indicating reload success or errors.


## UC-80: Debug commands from Kitu Shell (detailed flow)

### Architectural checkpoints

- Treat shell commands as regular events processed in ticks.

### Flow

1. Shell receives text command (e.g., `spawn_enemy goblin`).
2. Convert to `/debug/*` event and enqueue.
3. Backend systems handle it during the tick, sharing pipelines with normal gameplay.
4. Return results via `/debug/log` or `/ui/notice`.


## UC-81: State monitoring in Web Admin (detailed flow)

### Architectural checkpoints

- Expose runtime state via WebSocket without side effects.

### Flow

1. Web Admin backend connects to KituRuntime (local channel/WS).
2. Periodically fetches ECS snapshots, logs, metrics.
3. Serves data to the Web UI; backend never mutates game state unless via explicit debug events.


## UC-82: Replay (input playback) (detailed flow)

### Architectural checkpoints

- Deterministic playback based on recorded inputs per tick.

### Flow

1. Recorder logs `/input/*` events with tick timestamps.
2. Replay runner feeds the inputs back into KituRuntime at the same tick cadence.
3. Outputs (render/UI/debug events) should match the original run deterministically.


## UC-83: Running Kitu Shell commands from Web Admin (detailed flow)

### Architectural checkpoints

- Reuse the same debug-event channel; Web Admin is just another client.

### Flow

1. Web UI posts a command → backend forwards as `/debug/*` to runtime.
2. Runtime processes it in the tick loop alongside Shell commands.
3. Responses stream back over WebSocket for display.


## UC-90: Save / load (detailed flow)

### Architectural checkpoints

- Keep serialization/deserialization deterministic and versioned.

### Flow

1. **Save**: serialize necessary ECS components/resources (player state, quests, inventory, flags) to a versioned format (e.g., SQLite/MessagePack file).
2. **Load**: pause runtime, rebuild world from the save data, then resume ticks.
3. **Validation**: handle schema migrations or missing data gracefully; emit UI notices on success/failure.

