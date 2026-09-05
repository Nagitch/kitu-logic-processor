# Unity-only endless arena: implementation pain log

Date: 2026-09-05

Related: [Issue #111](https://github.com/Nagitch/kitu-logic-processor/issues/111),
[game specification](specs/unity-only-arena-game.md),
[verification](specs/unity-only-arena-verification.md).

These observations come from implementing the Unity-only arena in this worktree.
Kitu has not been introduced. The proposed improvements below are hypotheses;
the current C# implementation and passing tests are the comparison baseline.
Screens, art, font sizing and native-window presentation are Unity work and are
not counted as evidence for moving game rules into Kitu.

## 1. Death, rewards and recovery require a shared update order

### Context

The original `UnityOnlyActionRpg/Runtime/ActionRpgPlayerController.cs` applies
damage and immediately calls `RegisterPlayerDeath`. `ActionRpgEnemy.Die` calls
`RegisterEnemyDefeated`, which updates XP, spawns a potion and evaluates victory.
Both paths are driven by their own MonoBehaviour callbacks and Unity clocks.
The new spec requires a last-enemy/player simultaneous death to count the kill
but suppress floor clear, the boss reward and full healing.

### Pain

Reusing independent immediate callbacks does not express that shared outcome.
The new implementation had to queue damage, resolve enemy deaths, apply player
damage through shields, then choose death before clear/reward. Shield recovery
must observe the same damage update. This order was explicitly coordinated
between the simulation and inventory work and covered by separate boundary tests.

### Evidence

- `EndlessArena/Runtime/ArenaSimulation.cs`: `Step`, `ResolveDamage`, `ClearFloor`.
- `ArenaSimulationTests.SimultaneousPlayerAndLastEnemyDeathRecordsKillButNeverClearsOrRewards`
  covers both a normal enemy and a boss.
- `BossClearHealsOnceAndKeepsRewardContentsAndShieldCharge` and
  `DeathSnapshotStaysFixedAndRetryResetsRunWhileMenuClearsIt` cover downstream effects.
- The original smoke scripts and scene remain unchanged for comparison.

### Kitu Hypothesis

A shared runtime tick, ordered events and inspectable state could make the
damage-to-death-to-reward sequence easier to inspect and reproduce without
reconstructing a Unity scene. Compare the same simultaneous-hit sequence and
its output/state snapshots after a future migration. The gameplay priority
still has to be designed; moving code to Rust alone is not an improvement.

### Candidate Migration Slice

Damage resolution, HP/death, floor-clear/reward transitions and recovery timing.
Unity retains input, actors, hit feedback, camera and HUD.

## 2. Shields and inventory have different state lifetimes

### Context

Each shield retains its own charge and fractional recovery, while the recovery
delay belongs to the player and persists even without an equipped shield.
Backpack/chest/equipment moves must keep the same item; failed moves must be atomic.

### Pain

Implementing the buttons was only one part of this work. The rules needed
ownership-preserving swap operations, type/capacity guards, item conservation
checks, and a protocol of `ApplyDamage` followed by exactly one `AdvanceTime`
per running update. UI pause and floor transition must suppress that clock.
Recreating a shield during a move would silently refill it.

### Evidence

- `EndlessArena/Runtime/ArenaInventory.cs`: item instance state, equipment/chest
  transactions, shared recovery delay and damage-update suppression.
- `TakingSwappingAndEquippingConserveItemInstancesEvenWhenBackpackIsFull`.
- `InvalidOperationsAreAtomicAndUnequipNeedsAnEmptyBackpackSlot`.
- `ShieldChargeAndFractionSurviveSwapsButOnlyEquipmentRegenerates`.
- `DamageWithoutAShieldStillStartsSharedDelayAndChangingEquipmentKeepsIt`.
- Fifteen focused inventory cases pass in the real Unity EditMode runner.

### Kitu Hypothesis

Authoritative item locations and inspectable player/item state could make a
failed swap or unexpected shield charge explainable through a single snapshot
and repeatable command sequence. A future comparison should measure setup and
inspection effort for the same swap/damage/pause sequence. Kitu would not remove
the need to specify capacities, legal moves or which timer belongs to whom.

### Candidate Migration Slice

Item identity/location, inventory commands, equipment effects and recovery state.
Unity retains list layout, selection and button/input presentation.

## 3. Input integration tests needed an Editor-specific timing fixture

### Context

The rule tests passed, but we also needed proof that actual Input System events
reach `ArenaGame.Update`: WASD, both mouse buttons, Z/X, E, Tab and Esc.

### Pain

The first PlayMode run passed 2 of 9 cases and failed the 7 input-dependent
cases. Esc/Tab/E edges were absent and 240-frame waits completed in roughly
0.05–0.10 seconds without the expected state. Inspecting the installed Input
System showed that Editor updates remain active in manual mode and background
GameView focus can route or suppress device input.

The fixture needed temporary focus/background settings, input injection in a
test-only early Update, a matching LateUpdate checkpoint, and real-time rather
than frame-count timeouts. All temporary settings/devices/preferences are restored.
The test still uses the real production `ArenaGame.Update`; it does not invoke
that method by reflection or substitute an alternate gameplay path.

### Evidence

- `EndlessArena/Tests/PlayMode/ArenaFrontendTests.cs`: fixture and input driver.
- `Logs/arena-playmode-first-attempt.xml`: 2 passed / 7 failed, 0.2975 seconds.
- `Logs/arena-playmode.xml`: rerun of those 9 cases passed in Unity 6000.5.0f1.
- The standalone Start button and Esc pause worked during native UI checks;
  the failure was in the test input fixture, not a reason to bypass the real input path.

### Kitu Hypothesis

Runtime input events and a replay/scenario runner could verify many input-to-rule
sequences without Editor focus and frame scheduling. The Unity bridge would
still require real Input System/UI tests; Kitu cannot replace fullscreen, cursor,
button or physical keyboard verification. Separate those responsibilities and
compare the cost of reproducing an identical gameplay command sequence.

### Candidate Migration Slice

Gameplay command ingestion, pause/tick ownership and scenario state inspection;
keep the Unity input adapter and its small integration suite.

## Counterevidence to preserve

The Unity-only rules were separated into ordinary C# and can already be tested
without rendering a scene. The stock-loadout scenario uses real chest transactions,
movement, aim and attacks to clear 11 floors without modifying HP, enemy positions
or weapon statistics. This is useful capability already achieved without Kitu.
Future migration evidence must improve on this baseline, rather than compare
against an artificially untestable Unity implementation.
