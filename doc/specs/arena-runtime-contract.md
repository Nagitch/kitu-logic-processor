# Endless Arena runtime contract, version 1

Status: accepted migration contract. Stage 1 implements the C# reference recorder,
detached state projection and fixtures. Arena Rust handlers, network adapters,
domain-event emission and a live Unity recording UI are subsequent stages; this
document does not claim those features already exist. Delivery is tracked in #129,
with the executable reference boundary in #130 and before/after evidence in #111.

## Authority and baseline

The reference is PR #128, commit `38f2b4be4b7b2b604f1b21dfe9ce407846e0fc43`.
`ArenaSimulation` and `ArenaInventory` retain their original rules, f32 arithmetic,
iteration order and 1/60-second timestep. Serializable attributes and partial-class
diagnostic methods do not alter those rules. Source hashes are pinned in
`kitu-integration-runner/scenarios/arena/reference/baseline.json`.

Unity owns device input, camera-relative movement conversion, screen-to-ground
aim conversion, presentation, audio and display settings. Kitu owns movement,
collisions, attacks, damage, inventory, AI, progression, interaction eligibility,
pause and gameplay clocks. World coordinates use X/Z, represented by C# Vector2
x/y; distances are arena units, speed is units/second, time is seconds, and health
and damage are integers. Item and entity IDs are positive, stable within a run,
and scoped by the runtime session and run. Inventory empty slots use item ID 0.

The existing `/input/move` delta-translation API is unchanged. Arena uses the
separate `/input/arena/*` namespace below. Generic Kitu crates own execution and
adapters; the demo application owns Arena-specific commands, state and rules.

## Logical inputs

Every discrete command carries contract version, session, message ID and ordered
payload. The receiver assigns a monotonically increasing accepted sequence and
actual application tick. Clients cannot set authority by supplying a tick hint.
Deduplication is scoped to the runtime session (including retries/new runs): the
same ID and payload returns the original accepted/rejected result without another
mutation; reuse with a different payload returns `id_conflict`. Record duplicate
attempts for diagnostics. Source/session identities are adapter metadata, not OSC
address components.

The following payload columns define **ordered** OSC arguments; metadata is in
the envelope. Types are `i` = i32, `f` = finite f32, `b` = boolean. Slots are
WeaponA=0, WeaponB=1, ItemA=2, ItemB=3; backpack indices are 0..2. A command's item
ID identifies the expected current item, preventing stale UI indices from moving
a replacement item.

| Address suffix under `/input/arena/` | Payload | Eligibility/result |
| --- | --- | --- |
| `start` | none | Opening or results; resets run, enters preparation |
| `menu` | none | Clears run and returns to opening |
| `frame` | move_x:f, move_z:f, has_aim:b, aim_x:f, aim_z:f, fire_a:b, fire_b:b | Latest continuous control state; move normalized/clamped by simulation |
| `use` | slot:i | One press for ItemA/B; ignored empty slot, shield remains automatic; consumed by the next gameplay step only |
| `pause` | none | Active run; stops gameplay clocks |
| `resume` | none | Paused active run; explicit resume |
| `disconnect` | none | Host-generated controller-loss input; pauses and clears held/queued controls |
| `inventory` | none | Safe phase with no overlay; opens paused inventory |
| `chest` | none | Safe phase, no overlay, chest exists and distance <=2; opens paused chest |
| `close` | none | Closes inventory/chest; host resets held-button release gate |
| `take` | item_id:i, backpack_index:i | Chest open; take/swap the identified chest item |
| `equip` | item_id:i, backpack_index:i, equipment_slot:i | Inventory/chest open; original type/capacity rules |
| `unequip` | item_id:i, equipment_slot:i | Inventory/chest open; requires free backpack slot |
| `discard` | item_id:i, backpack_index:i | Inventory/chest open; discards exactly that item |
| `upgrade` | item_id:i, backpack_index:i | Inventory/chest open; consumes upgrade |

Other gameplay changes are not accepted as client-provided outcomes: no inbound
"enemy died", damage totals, clear rewards or arbitrary authoritative snapshots.
Screen settings remain Unity-local; entering settings during a run occurs from
pause, and therefore needs no additional Arena clock. Real Input System tests
remain responsible for focus loss and button release gating.

Malformed types/coordinates are rejected before committing the batch. A valid
but ineligible command is rejected without changing gameplay state; other valid
commands in the tick still execute in sequence. Reference outcome codes are
`ok`, `invalid_state`, `out_of_range`, `stale_item`, `invalid_target`,
`rule_rejected`, `unknown_command`, and `id_conflict`. Localized display strings
are not the stable command contract. Do not compare error text as game state.

## Tick and pause semantics

The host advances a single Runtime at 60 Hz independently of input traffic.
Messages received during tick N are eligible at N+1, following the existing
runtime input/output barriers. Replay supplies the already-committed batch for
the recorded application tick; do not add a second transport delay.

1. Validate and commit the ordered control-command batch.
2. Apply commands in order and form effective continuous controls. Coalesce
   repeated `use` presses for the same slot to one use in this gameplay step,
   matching the original C# queued bool. Consumed edges never remain held.
3. If opening, results or an overlay is active, do not advance gameplay.
4. Otherwise execute exactly one C#-equivalent simulation step:
   transition countdown (and early return while transitioning); elapsed/effect
   aging/cooldowns; movement then aim; consumable A then B; weapon A then B;
   enemies in spawn order; projectiles; grenades; queued damage and deaths;
   shield clock once; player death before clear/reward; portal overlap.
5. Emit command outcomes, domain events and the completed state projection only
   after the update barrier. Then advance runtime tick.

Runtime ticks and completed simulation steps are distinct. Paused ticks still
process commands but leave all gameplay timers and transforms unchanged. There
is no catch-up for time spent paused. Transition steps do not advance run elapsed
time, matching the reference. Preserve last-enemy/player simultaneous death:
record the enemy kill, but never clear the floor, reward or heal the dead player.
Clear/transition removes transient attacks. Portal arming/re-entry prevents
duplicate transitions.

On controller loss the host commits disconnect, clears held controls and pauses.
Reconnection delivers a full projection, does not replay unacknowledged consumed
inputs, and requires explicit resume and fresh button presses. Runtime shutdown
is not a resumable save-state in v1.

## Output contracts

All outputs identify their session/run, source tick and output order. This is a
logical contract; JSON, MessagePack and FFI encoding must preserve it.

| Family | Stable content |
| --- | --- |
| `/ui/arena/command` | message ID, applied tick, accepted, duplicate, reason code |
| `/game/arena/attack` | actor/attack ID, weapon/item ID, origin, direction |
| `/game/arena/damage` | source/target ID, damage, shield absorption, resulting HP |
| `/game/arena/death` | entity ID and kind; emitted once |
| `/game/arena/inventory` | operation and affected item IDs/locations |
| `/game/arena/phase` | previous/new phase, floor, clear/kill counters |
| `/game/arena/reward` | floor and generated item IDs; emitted once |
| `/game/arena/result` | immutable terminal run result |
| `/render/arena/*` | object identity, spawn/despawn, transforms and presentation effects |
| `/ui/arena/state` | phase, overlay, HP/shields, equipment/backpack/chest and terminal result |

Stage 1 fixtures freeze command outcomes and state checkpoints, not invented
domain events. Each migration slice adds event assertions at the actual rule
emission points. Snapshots alone do not prove the ordering of transient attacks.

## Reference trace and comparison

`ArenaReferenceSession` is an opt-in plain-C# boundary adapter around the existing
rules. Production `ArenaGame` and its Input System/scene tests remain unchanged.
The adapter validates commands and pause/interaction guards without running a
scene. It neither exports nor imports mutable ECS/scene state as an input.

Each `scenario.json` declares schemaVersion=1, baselineRevision, tickRate=60,
scenarioId and contiguous `steps` starting at tick 0. Each step contains ordered
commands and an **effective frame**, after input sampling/coalescing. The frame
uses the original ArenaInput fields (Move, HasAim, AimPoint, FireA/B, UseA/B).
Move/Aim/Fire map to `frame`; true UseA/B map to tick-local `use` intents in A/B
order. This normalized fixture is not a raw packet/device log. Missing frames
are not permitted; a neutral frame is explicit. Control IDs stay unchanged on
retry. Replay uses the captured frame, never reruns the bot's targeting decisions.

`expected.ndjson` stores one complete detached state per checkpoint: all control
ticks, discrete changes and at least one checkpoint every 60 ticks. It includes
private timers, portal arming, allocation counters, shield fractions and result
data. Lists preserve the reference iteration order; empty inventory slots are
explicit ID-0 objects. Snapshots do not serve as restore points. `outcomes.ndjson`
stores every command outcome in execution order. Its `tick` is the observation
tick; `appliedTick` is the first application/evaluation tick, retained on duplicate
delivery (-1 for conflicting ID reuse). Thus retries cannot invent a later
application time.

The same-C# frozen-fixture test requires exact values. Cross-language comparison
requires exact IDs, HP, shield charge, inventory locations, enums, flags, counters,
array membership/order, outcomes and event ticks. Finite floating-point state
uses absolute error <=1e-4; this never excuses a different collision, damage,
death or phase transition. On mismatch report the first tick and field path.
The gameplay reference must not be rewritten to make the Rust implementation pass.

Runs have fixed spawning and chest contents; v1 needs no random seed. Later
content integration stores the evaluated content and script bytes/hashes plus
runtime/schema versions with the record. Incompatible or unavailable versions
must fail explicitly. Editor-only current settings and wall-clock timestamps
are not gameplay equivalence fields.

## Reproduction

From the Unity project, execute `UnityOnlyArena.Tests.ArenaReferenceFixtureBuilder.Export`
with the licensed Unity 6000.6.0f1 Editor. It writes to ignored
`Logs/arena-reference/` by default (override `KITU_ARENA_REFERENCE_OUTPUT` explicitly).
Normal tests only read the checked-in fixtures. Inspect generated differences
before replacing a fixture. Run the `UnityOnlyArena.Tests` EditMode assembly and
the existing PlayMode tests; use `tools/verify-arena-reference.py` from the
repository root to verify pinned source provenance and fixture structure.

## Stage 2 concrete connection

`apps/demo-game` installs the persistent Arena application; the admin host clocks
it independently at 60 Hz. `KituEndlessArena.unity` is the separate migration
scene. This slice supports start/menu, movement/aim, pause/resume and host-originated
disconnect. Inventory, combat and progression are later slices. Structurally
valid unmigrated commands currently return `not_yet_implemented`.

Connect to `/ws/runtime`. Initial text events include `arenaSession` with `id` and
`schemaVersion`, followed by `/ui/arena/state`. A controlling client sends:

```json
{"schemaVersion":1,"sessionId":"<arenaSession.id>","clientId":"<stable-client-id>","messageId":1,"address":"/input/arena/start","args":[]}
```

`args` is an ordered array of `{ "type": "float", "value": 1.0 }` (or `int`,
`int64`, `bool`, `str`). Version/session mismatches and malformed envelopes are
rejected before queue admission. One websocket/client identity owns controls;
other connected clients can observe. The host reserves producer IDs beginning
with `host:` and generates disconnect when the owning websocket closes. Losing
an observer does not pause the game. Restarting the process changes its session
ID. A reconnect sends a complete projection and requires explicit resume; clients
must not automatically resend unacknowledged commands or held controls.

State and command output bodies are encoded as one OSC string argument containing
JSON. State uses the reference field names, including Vector2 `x`/`y`. A command
outcome also includes producer `source` and original runtime admission `sequence`;
retries preserve the original applied tick, sequence and outcome. Continuously
sampled frames share a per-producer high-water ID with discrete commands. Stale
frames are ignored; a frame reusing a cached command ID returns `id_conflict`.
A first-seen discrete ID at or below that mark also returns `id_conflict`, so a
previous frame ID cannot become a consuming operation. Exact cached command
retries still return the original result. This keeps continuous input memory
bounded without permitting cross-kind reuse. IDs must be monotonically allocated
across all kinds; an out-of-order, previously unseen discrete command is rejected.
Deferred commands still validate their full declared argument shape before queue
admission; genuinely unknown addresses fail admission.

JSON is the Arena network encoding for this stage. The pre-existing legacy KEP
path remains supported for legacy OSC; versioned Arena MessagePack admission is
stage 15. Per-entity render/domain events and complete inventory state will be
added at their migration slices. The current state projection is the rendering
source for the initial player view, and does not claim full-game parity.
