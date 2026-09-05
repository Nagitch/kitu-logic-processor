# Endless Arena runtime contract, version 1

Status: stages 1–4 are merged; stage 5 completes the original game's Kitu path
and makes its Unity scene the default. Delivery is tracked in #129, with frozen
reference evidence in #130 and the Unity-first value investigation in #111.

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

## Stage 3 inventory implementation

The Arena application now owns the complete `inventory` projection: backpack,
equipment, chest, item IDs, HP/capacity, attack upgrades, shield charge/fractions
and shared recovery timers. Inventory/chest overlays pause gameplay; start resets
items and creates the preparation chest, while menu clears ownership. Safe-phase
commands use expected item IDs and the declared typed payloads. Out-of-range
backpack destinations for `take` return `invalid_target` before any transfer,
consistent with other invalid target indices (late review of #131, fixed in #134).
The pinned C# rule files and both previously frozen traces remain unchanged.

Successful inventory mutations emit `/game/arena/inventory` once, before the
corresponding command receipt in the same ordered output bundle. Its single JSON
string contains `tick`, admission `sequence`, producer `source`, `messageId`,
`operation` (full OSC address), requested `itemId`/`index`/`slot`, and the complete
post-operation inventory. Rejected commands and duplicate deliveries do not emit
another inventory event. This preserves the identity of consumed as well as
transferred items; the full projection also identifies any outgoing swapped item.

The inventory rule API includes medkit/grenade consumption eligibility and shield
damage/recovery for direct rule comparison. Combat-driven damage and the `use`
input's attack/target resolution are stage 4. There is no public network command
that sets HP, invents damage or restores a mutable inventory snapshot. The shared
shield clock advances exactly once on each unpaused gameplay step; only equipped
shields recover. Stored fractions survive backpack/chest transfers and upgrades.

The differential test covers every migrated state field, the complete inventory
object, and every command outcome in `preparation` (28 ticks) and the stock
loadout/approach before its first floor transition (185 ticks). It replays the
recorded effective frames, with a separate generated frame producer to preserve
the original recorded discrete IDs. Floats use 1e-4 absolute tolerance; item IDs,
HP, shield integers, slots, flags, order and result codes are exact. Full combat
state and progression parity are later gates, not claimed by this slice.

## Stage 4 combat implementation

The authoritative projection now has every C# checkpoint field, including enemies,
projectiles, grenades, effects, shared entity allocation, both weapon cooldowns,
HP/shield timers and the terminal result. The first floor is playable over the
existing connection; the portal after that floor is enabled in stage 5. The
Unity client renders projected actors without constructing an ArenaSimulation.

`use` acknowledges a queued intent, not guaranteed consumption. At the gameplay
step, repeated intents for one slot coalesce into one attempt. `/ui/arena/use`
reports `slot`, `itemId`, `consumed` and `code`: `ok`, `empty_slot`,
`automatic_shield`, `full_health` or `invalid_aim`. Pause/menu/start/overlay
operations clear queued intents; transition steps discard them as in C#.
Successful consumption emits one inventory event with operation `/input/arena/use`, item/slot
and the post-consumption inventory. This event belongs to the coalesced gameplay
step, so it has tick/order instead of an individual command admission identity.
A duplicate intent receipt does not enqueue another use. Unity sends the sampled
frame before a Z/X press and requires release after UI changes/reconnection.

Combat emits attack, damage, death and phase/result events at their rule execution
points, plus spawn/despawn records for projected actors. Each combat event has
`tick` and `order` within the emitted OSC bundle. Enemy damage aggregates per
target in first-hit order and records its attack IDs; enemy deaths traverse the
enemy list in reverse, followed by incoming player damage. Player death precedes
floor clear, and prevents rewards. The runtime transport envelope identifies the
session. A run is currently delimited by successful start commands; durable run
identity and versioned recording metadata are part of the recording stage.

The frozen stock input prefix (470 ticks) compares **all** state fields, checking
both object key sets and array order, through the first clear. Edge regressions
follow the original C# arrangements for simultaneous death, cone/cooldown,
swept nearest hits/initial overlap/range/walls, grenade targeting/delay, medkit
ordering and boss telegraph/recovery. Explicit event-order assertions supplement
the checkpoint oracle; the baseline does not contain a raw C# domain-event log.
The PlayMode connection test covers real Input System press gating, grenade
consumption/pause, projected enemies and a first-floor clear over WebSocket.

## Stage 5 full-game implementation

The default enabled scene is `KituEndlessArena.unity`. `EndlessArena.unity` remains
checked in as an explicitly selected Unity-only reference. The client displays
objectives, all equipment/shield values, terminal results and retry, and reuses
Unity-local volume/fullscreen preferences. Settings can open from the opening
screen or an already-paused run. Editing/canceling a draft never resumes the run;
management ticks continue while the authoritative gameplay clock stays paused.

Normal floors contain min(floor+2, 12) enemies, with the reference pursuer/shooter/
heavy split and ordered spawn positions. Every fifth floor contains one boss.
After a living boss clear, the existing inventory restores HP (not shield charge)
and creates the repeating reward chest once. `/game/arena/reward` reports `tick`,
`order`, `floor`, restored `health`, ordered `itemIds` and the complete resulting
`inventory`. Continuing in the cleared phase cannot regenerate rewards or heal
again. Clearing with the player overlapping the portal requires exit/re-entry.
The next floor retains HP, item identities and shield recovery time, clears
transient attacks and resets entry position/weapon cooldowns. There is no victory
endpoint at 10F, 15F or 20F.

The full frozen stock recording (5,528 input ticks) matches all 550 checkpoints
and 53 command receipts, including 11F entry, natural death at tick 5526 and retry
at tick 5527. Reward events occur exactly on the C# boss-clear checkpoint ticks;
there are two rewards and one terminal result. Retry emits one Results→Preparing
transition. Separate reference-derived rule regressions cover all 21 floor
rosters, scaling, boss reward one-time behavior and portal re-entry. The full
Unity scene test additionally plays a live, feedback-driven stock run over the
network; it is distinct from the deterministic recorded-input oracle.
