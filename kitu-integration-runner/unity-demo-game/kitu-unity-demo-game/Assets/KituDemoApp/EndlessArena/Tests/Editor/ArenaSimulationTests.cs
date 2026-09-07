using System;
using System.Linq;
using NUnit.Framework;
using UnityEngine;

namespace UnityOnlyArena.Tests
{
    public sealed class ArenaSimulationTests
    {
        private const float Tick = 1f / 60f;

        [Test]
        public void OpeningDoesNotRunAndStartingCreatesOnePreparationChest()
        {
            var game = new ArenaSimulation();
            game.Step(3f, new ArenaInput { FireA = true, Move = Vector2.up });
            Assert.That(game.Phase, Is.EqualTo(ArenaPhase.Opening));
            Assert.That(game.Elapsed, Is.Zero);
            game.StartRun();
            Assert.That(game.Floor, Is.Zero);
            Assert.That(game.Phase, Is.EqualTo(ArenaPhase.Preparing));
            Assert.That(game.Enemies, Is.Empty);
            Assert.That(game.ChestAvailable && game.PortalAvailable, Is.True);
            Assert.That(game.Inventory.Chest.Count, Is.EqualTo(11));
            game.StartRun();
            Assert.That(game.Inventory.Chest.Count, Is.EqualTo(11));
            Assert.That(game.PlayerPosition, Is.EqualTo(ArenaSimulation.EntrancePosition));
        }

        [Test]
        public void EndlessProgressionThroughTwentyOneFloorsUsesSpecifiedRosterAndRewards()
        {
            var game = Started();
            int expectedKills = 0;
            int bosses = 0;
            for (int floor = 1; floor <= 21; floor++)
            {
                EnterNext(game);
                Assert.That(game.Floor, Is.EqualTo(floor));
                Assert.That(game.Phase, Is.EqualTo(ArenaPhase.Combat));
                Assert.That(game.Inventory.Chest, Is.Empty);
                Assert.That(game.PortalAvailable || game.ChestAvailable, Is.False);
                bool boss = floor % 5 == 0;
                int expectedCount = boss ? 1 : Mathf.Min(floor + 2, 12);
                Assert.That(game.Enemies.Count, Is.EqualTo(expectedCount));
                if (boss)
                {
                    Assert.That(game.Enemies[0].Kind, Is.EqualTo(ArenaEnemyKind.Boss));
                    Assert.That(game.Enemies[0].MaxHealth, Is.EqualTo(Mathf.CeilToInt(300 * (1 + 0.12f * (floor - 1)))));
                    bosses++;
                }
                else
                {
                    Assert.That(game.Enemies.Count(e => e.Kind == ArenaEnemyKind.Shooter), Is.EqualTo(floor == 1 ? 0 : expectedCount / 3));
                    Assert.That(game.Enemies.Count(e => e.Kind == ArenaEnemyKind.Heavy), Is.EqualTo(floor <= 2 ? 0 : expectedCount / 4));
                    ArenaEnemy pursuer = game.Enemies.First(e => e.Kind == ArenaEnemyKind.Pursuer);
                    Assert.That(pursuer.MaxHealth, Is.EqualTo(Mathf.CeilToInt(40 * (1 + 0.12f * (floor - 1)))));
                    Assert.That(pursuer.Damage, Is.EqualTo(Mathf.FloorToInt(10 * (1 + 0.08f * (floor - 1)))));
                }
                expectedKills += expectedCount;
                EliminateRosterWithInput(game);
                Assert.That(game.Phase, Is.EqualTo(ArenaPhase.Cleared));
                Assert.That(game.FloorsCleared, Is.EqualTo(floor));
                Assert.That(game.EnemiesDefeated, Is.EqualTo(expectedKills));
                Assert.That(game.BossesDefeated, Is.EqualTo(bosses));
                Assert.That(game.ChestAvailable, Is.EqualTo(boss));
                Assert.That(game.Inventory.Chest.Count, Is.EqualTo(boss ? 11 : 0));
            }
            Assert.That(game.Result, Is.Null, "There is no victory or endpoint at 10F/15F.");
        }

        [TestCase(1)]
        [TestCase(5)]
        public void SimultaneousPlayerAndLastEnemyDeathRecordsKillButNeverClearsOrRewards(int floor)
        {
            var game = AtFloor(floor);
            ArenaEnemy enemy = KeepOneEnemy(game);
            enemy.Position = game.PlayerPosition + Vector2.up;
            enemy.Health = 20;
            enemy.Damage = 1000;
            enemy.AttackCooldown = 0f;
            enemy.BossState = ArenaBossState.Pursuit;
            enemy.PhaseRemaining = 3f;
            game.Inventory.Equipment[0] = new ArenaItem(10000, "Blade", ItemKind.Blade, 20, 0.5f);
            int previousKills = game.EnemiesDefeated;
            int previousBosses = game.BossesDefeated;
            game.Step(Tick, new ArenaInput { HasAim = true, AimPoint = enemy.Position, FireA = true });
            Assert.That(game.Phase, Is.EqualTo(ArenaPhase.Results));
            Assert.That(game.Inventory.Health, Is.Zero);
            Assert.That(game.EnemiesDefeated, Is.EqualTo(previousKills + 1));
            Assert.That(game.BossesDefeated, Is.EqualTo(previousBosses + (floor == 5 ? 1 : 0)));
            Assert.That(game.FloorsCleared, Is.EqualTo(floor - 1));
            Assert.That(game.ChestAvailable || game.PortalAvailable, Is.False);
            Assert.That(game.Inventory.Chest, Is.Empty);
            Assert.That(game.Result.Floor, Is.EqualTo(floor));
            float elapsed = game.Result.Elapsed;
            game.Step(10f, new ArenaInput { FireA = true, UseA = true, Move = Vector2.up });
            Assert.That(game.Elapsed, Is.EqualTo(elapsed));
            Assert.That(game.EnemiesDefeated, Is.EqualTo(previousKills + 1));
            Assert.That(game.Projectiles, Is.Empty);
        }

        [Test]
        public void BossClearHealsOnceAndKeepsRewardContentsAndShieldCharge()
        {
            var game = AtFloor(5);
            game.Inventory.ApplyDamage(60);
            var shield = new ArenaItem(9000, "Shield", ItemKind.Shield, shield: 12);
            game.Inventory.Equipment[(int)EquipmentSlot.ItemA] = shield;
            EliminateRosterWithInput(game);
            Assert.That(game.Inventory.Health, Is.EqualTo(100));
            Assert.That(shield.Shield, Is.EqualTo(12));
            int firstItem = game.Inventory.Chest[0].Id;
            Assert.That(game.Inventory.TakeChest(0, 0), Is.True);
            game.Inventory.ApplyDamage(20);
            game.Step(Tick, default(ArenaInput));
            Assert.That(game.Inventory.Health, Is.EqualTo(92), "Remaining in the cleared state cannot reapply boss healing.");
            Assert.That(game.Inventory.Chest.Count, Is.EqualTo(10));
            Assert.That(game.Inventory.Backpack[0].Id, Is.EqualTo(firstItem));
            Assert.That(game.BossesDefeated, Is.EqualTo(1));
            Assert.That(game.FloorsCleared, Is.EqualTo(5));
        }

        [Test]
        public void PortalAppearingUnderPlayerRequiresExitAndReentryAndCannotTransitionTwice()
        {
            var game = AtFloor(1);
            game.PlayerPosition = ArenaSimulation.PortalPosition;
            EliminateRosterWithInput(game);
            Assert.That(game.Phase, Is.EqualTo(ArenaPhase.Cleared));
            game.Step(Tick, default(ArenaInput));
            Assert.That(game.Phase, Is.EqualTo(ArenaPhase.Cleared));
            game.Step(0.4f, new ArenaInput { Move = Vector2.down });
            Assert.That(game.Phase, Is.EqualTo(ArenaPhase.Cleared));
            game.Step(0.4f, new ArenaInput { Move = Vector2.up });
            Assert.That(game.Phase, Is.EqualTo(ArenaPhase.Transition));
            Assert.That(game.TryAdvanceFloor(), Is.False);
            game.Step(ArenaSimulation.TransitionDuration, new ArenaInput { FireA = true });
            Assert.That(game.Floor, Is.EqualTo(2));
            Assert.That(game.Phase, Is.EqualTo(ArenaPhase.Combat));
            Assert.That(game.Enemies.Count, Is.EqualTo(4));
            Assert.That(game.PlayerPosition, Is.EqualTo(ArenaSimulation.EntrancePosition));
        }

        [Test]
        public void PortalRejectsNoWeaponWithoutDiscardingChest()
        {
            var game = Started();
            Assert.That(game.Inventory.Unequip(EquipmentSlot.WeaponA), Is.True);
            Assert.That(game.TryAdvanceFloor(), Is.False);
            Assert.That(game.Phase, Is.EqualTo(ArenaPhase.Preparing));
            Assert.That(game.Inventory.Chest.Count, Is.EqualTo(11));
            StringAssert.Contains("weapon", game.LastMessage);
        }

        [Test]
        public void IndependentWeaponsHitConeOncePerEnemyAndHonorCooldowns()
        {
            var game = AtFloor(1);
            game.PlayerPosition = Vector2.zero;
            var front = game.Enemies[0];
            var behind = game.Enemies[1];
            var outside = game.Enemies[2];
            Freeze(front, new Vector2(0f, 1f), 100);
            Freeze(behind, new Vector2(0f, -1f), 100);
            Freeze(outside, new Vector2(3f, 0f), 100);
            game.Inventory.Equipment[0] = new ArenaItem(9000, "Blade A", ItemKind.Blade, 20, 0.5f);
            game.Inventory.Equipment[1] = new ArenaItem(9001, "Blade B", ItemKind.Blade, 30, 1f);
            var input = new ArenaInput { HasAim = true, AimPoint = Vector2.up * 5f, FireA = true, FireB = true };
            game.Step(Tick, input);
            Assert.That(front.Health, Is.EqualTo(50));
            Assert.That(behind.Health, Is.EqualTo(100));
            Assert.That(outside.Health, Is.EqualTo(100));
            game.Step(Tick, input);
            Assert.That(front.Health, Is.EqualTo(50));
            for (int i = 0; i < 29; i++) game.Step(Tick, input);
            Assert.That(front.Health, Is.EqualTo(30), "A repeats after 0.5 seconds while B is still cooling down.");
        }

        [Test]
        public void ProjectileSweepHitsNearestTargetEvenWhenListOrderIsFarFirst()
        {
            var game = AtFloor(1);
            game.PlayerPosition = Vector2.zero;
            var far = game.Enemies[0];
            var near = game.Enemies[1];
            Freeze(far, Vector2.up * 5f, 100);
            Freeze(near, Vector2.up, 100);
            Freeze(game.Enemies[2], Vector2.right * 8f, 100);
            game.Inventory.Equipment[0] = new ArenaItem(9000, "Shooter", ItemKind.Shooter, 12, 0.25f);
            game.Step(0.5f, new ArenaInput { HasAim = true, AimPoint = Vector2.up * 9f, FireA = true });
            Assert.That(near.Health, Is.EqualTo(88));
            Assert.That(far.Health, Is.EqualTo(100));
            Assert.That(game.Projectiles, Is.Empty);
        }

        [Test]
        public void BulletSpawnOverlappingEnemyStillHitsInsteadOfSkippingCloseTarget()
        {
            var game = AtFloor(1);
            game.PlayerPosition = Vector2.zero;
            var enemy = KeepOneEnemy(game);
            Freeze(enemy, Vector2.zero, 100);
            game.Inventory.Equipment[0] = new ArenaItem(9000, "Shooter", ItemKind.Shooter, 12, 0.25f);
            game.Step(Tick, new ArenaInput { FireA = true });
            Assert.That(enemy.Health, Is.EqualTo(88));
            Assert.That(game.Projectiles, Is.Empty);
        }

        [Test]
        public void EnemyBulletSweepDamagesPlayerAndIgnoresOtherEnemies()
        {
            var game = AtFloor(1);
            game.PlayerPosition = Vector2.zero;
            foreach (var enemy in game.Enemies) Freeze(enemy, new Vector2(0f, -3f), 100);
            game.Projectiles.Add(new ArenaProjectile
            {
                Id = 10000, Position = new Vector2(0f, -5f), Direction = Vector2.up,
                Speed = 20f, DistanceRemaining = 20f, Damage = 8, EnemyOwned = true
            });
            game.Step(0.5f, default(ArenaInput));
            Assert.That(game.Inventory.Health, Is.EqualTo(92));
            Assert.That(game.Enemies.All(e => e.Health == 100), Is.True);
            Assert.That(game.Projectiles, Is.Empty);
        }

        [Test]
        public void ActualPursuerMovementAndMeleeEventuallyKillAnIdlePlayer()
        {
            var game = AtFloor(1);
            for (int i = 0; i < 30 * 60 && game.Phase != ArenaPhase.Results; i++)
                game.Step(Tick, default(ArenaInput));
            Assert.That(game.Phase, Is.EqualTo(ArenaPhase.Results));
            Assert.That(game.Inventory.Health, Is.Zero);
            Assert.That(game.EnemiesDefeated, Is.Zero);
            Assert.That(game.Result.Floor, Is.EqualTo(1));
        }

        [Test]
        public void ContactAloneDoesNoDamageAndEnemyWaitsInitialAttackInterval()
        {
            var game = AtFloor(1);
            ArenaEnemy enemy = KeepOneEnemy(game);
            enemy.Position = game.PlayerPosition;
            game.Step(0.5f, default(ArenaInput));
            Assert.That(game.Inventory.Health, Is.EqualTo(100));
            game.Step(0.5f, default(ArenaInput));
            Assert.That(game.Inventory.Health, Is.EqualTo(90));
        }

        [Test]
        public void ShooterApproachesAndFiresTowardPlayerAfterItsInitialDelay()
        {
            var game = AtFloor(2);
            ArenaEnemy shooter = game.Enemies.Single(e => e.Kind == ArenaEnemyKind.Shooter);
            game.Enemies.Clear();
            game.Enemies.Add(shooter);
            game.PlayerPosition = Vector2.zero;
            shooter.Position = new Vector2(0f, 6.1f);
            game.Step(Tick, default(ArenaInput));
            Assert.That(shooter.Position.y, Is.LessThan(6.1f));
            Assert.That(game.Projectiles, Is.Empty);
            for (int i = 0; i < 88; i++) game.Step(Tick, default(ArenaInput));
            Assert.That(game.Projectiles, Is.Empty);
            game.Step(Tick, default(ArenaInput));
            Assert.That(game.Projectiles.Count, Is.EqualTo(1));
            Assert.That(game.Projectiles[0].EnemyOwned, Is.True);
            Assert.That(game.Projectiles[0].Direction.y, Is.LessThan(-0.99f));
            for (int i = 0; i < 60; i++) game.Step(Tick, default(ArenaInput));
            Assert.That(game.Inventory.Health, Is.LessThan(100));
        }

        [Test]
        public void BulletsExpireAtWallOrMaximumRange()
        {
            var game = Started();
            game.PlayerPosition = new Vector2(9f, 0f);
            game.Inventory.Equipment[0] = new ArenaItem(9000, "Shooter", ItemKind.Shooter, 12, 0.25f);
            game.Step(Tick, new ArenaInput { HasAim = true, AimPoint = Vector2.right * 20f, FireA = true });
            Assert.That(game.Projectiles.Count, Is.EqualTo(1));
            game.Step(0.2f, default(ArenaInput));
            Assert.That(game.Projectiles, Is.Empty);
            game.Projectiles.Add(new ArenaProjectile
            {
                Id = 10000, Position = Vector2.zero, Direction = Vector2.up,
                Speed = 12f, DistanceRemaining = 1f, Damage = 1
            });
            game.Step(0.2f, default(ArenaInput));
            Assert.That(game.Projectiles, Is.Empty);
        }

        [Test]
        public void GrenadeRequiresAimConsumesAtThrowAndDealsSnapshotAreaDamageAfterDelay()
        {
            var game = AtFloor(1);
            game.PlayerPosition = Vector2.zero;
            var near = game.Enemies[0];
            var edge = game.Enemies[1];
            var outside = game.Enemies[2];
            Freeze(near, Vector2.up * 4f, 300);
            Freeze(edge, new Vector2(2f, 4f), 300);
            Freeze(outside, new Vector2(-8f, -8f), 300);
            var grenade = new ArenaItem(9000, "Grenade", ItemKind.Grenade);
            game.Inventory.Equipment[2] = grenade;
            game.Step(Tick, new ArenaInput { UseA = true });
            Assert.That(game.Inventory.Equipment[2], Is.SameAs(grenade));
            game.Step(Tick, new ArenaInput { UseA = true, HasAim = true, AimPoint = Vector2.zero });
            Assert.That(game.Inventory.Equipment[2], Is.SameAs(grenade));
            game.Step(Tick, new ArenaInput { UseA = true, HasAim = true, AimPoint = Vector2.up * 4f });
            Assert.That(game.Inventory.Equipment[2], Is.Null);
            Assert.That(game.Grenades.Count, Is.EqualTo(1));
            Assert.That(near.Health, Is.EqualTo(300));
            game.Inventory.Backpack[0] = new ArenaItem(9001, "Attack Upgrade", ItemKind.AttackUpgrade);
            Assert.That(game.Inventory.UseUpgrade(0), Is.True);
            game.PlayerPosition = near.Position; // The blast also overlaps the player.
            for (int i = 0; i < 28; i++) game.Step(Tick, default(ArenaInput));
            Assert.That(near.Health, Is.EqualTo(300));
            game.Step(Tick, default(ArenaInput));
            Assert.That(near.Health, Is.EqualTo(200), "Damage was fixed before the later upgrade.");
            Assert.That(edge.Health, Is.EqualTo(200));
            Assert.That(outside.Health, Is.EqualTo(300));
            Assert.That(game.Inventory.Health, Is.EqualTo(100));
            Assert.That(game.Grenades, Is.Empty);
            for (int i = 0; i < 30; i++) game.Step(Tick, default(ArenaInput));
            Assert.That(near.Health, Is.EqualTo(200), "An explosion cannot damage again on later frames.");
        }

        [Test]
        public void GrenadeTargetIsLimitedByThrowRangeAndArena()
        {
            var game = Started();
            game.PlayerPosition = Vector2.zero;
            game.Inventory.Equipment[2] = new ArenaItem(9000, "Grenade", ItemKind.Grenade);
            game.Step(Tick, new ArenaInput { UseA = true, HasAim = true, AimPoint = Vector2.right * 100f });
            Assert.That(game.Grenades[0].Target.x, Is.EqualTo(8f).Within(0.0001f));
            game.PlayerPosition = new Vector2(9f, 0f);
            game.Inventory.Equipment[2] = new ArenaItem(9001, "Grenade", ItemKind.Grenade);
            game.Step(Tick, new ArenaInput { UseA = true, HasAim = true, AimPoint = Vector2.right * 100f });
            Assert.That(game.Grenades[1].Target.x, Is.LessThan(ArenaSimulation.ArenaHalfExtent));
            Assert.That(game.Grenades[1].Target.x, Is.GreaterThan(9f));
        }

        [Test]
        public void MedkitInputsAreOrderedBeforeIncomingDamageAndSecondFullHealthMedkitIsKept()
        {
            var game = AtFloor(1);
            var enemy = KeepOneEnemy(game);
            Freeze(enemy, game.PlayerPosition + Vector2.up, 100);
            enemy.AttackCooldown = 0f;
            game.Inventory.ApplyDamage(50);
            game.Inventory.Equipment[2] = new ArenaItem(9000, "Medkit A", ItemKind.Medkit);
            var second = new ArenaItem(9001, "Medkit B", ItemKind.Medkit);
            game.Inventory.Equipment[3] = second;
            game.Step(Tick, new ArenaInput { UseA = true, UseB = true });
            Assert.That(game.Inventory.Health, Is.EqualTo(90));
            Assert.That(game.Inventory.Equipment[2], Is.Null);
            Assert.That(game.Inventory.Equipment[3], Is.SameAs(second));
        }

        [Test]
        public void BossTelegraphsThenFiresEightShotsAndDoesNotMeleeDuringTellOrRecovery()
        {
            var game = AtFloor(5);
            var boss = game.Enemies[0];
            boss.Position = Vector2.zero;
            boss.Speed = 0f;
            boss.AttackCooldown = 0f;
            boss.PhaseRemaining = Tick;
            game.PlayerPosition = Vector2.right;
            game.Step(Tick, default(ArenaInput));
            Assert.That(boss.BossState, Is.EqualTo(ArenaBossState.Telegraph));
            Assert.That(game.Inventory.Health, Is.EqualTo(100));
            for (int i = 0; i < 47; i++) game.Step(Tick, default(ArenaInput));
            Assert.That(game.Projectiles, Is.Empty);
            Assert.That(game.Inventory.Health, Is.EqualTo(100));
            game.Step(Tick, default(ArenaInput));
            Assert.That(boss.BossState, Is.EqualTo(ArenaBossState.Recovery));
            Assert.That(game.Projectiles.Count, Is.EqualTo(8));
            game.Projectiles.Clear(); // Isolate recovery's melee suppression from the fired projectiles.
            for (int i = 0; i < 59; i++) game.Step(Tick, default(ArenaInput));
            Assert.That(game.Inventory.Health, Is.EqualTo(100));
            game.Step(Tick, default(ArenaInput));
            Assert.That(boss.BossState, Is.EqualTo(ArenaBossState.Pursuit));
            Assert.That(game.Inventory.Health, Is.EqualTo(100));
            game.Step(Tick, default(ArenaInput));
            Assert.That(game.Inventory.Health, Is.LessThan(100));
        }

        [Test]
        public void FloorTransitionCarriesHealthInventoryAndShieldTimersButResetsTransientState()
        {
            var game = Started();
            game.Inventory.Backpack[0] = new ArenaItem(9000, "HP", ItemKind.HealthUpgrade);
            Assert.That(game.Inventory.UseUpgrade(0), Is.True);
            game.Inventory.ApplyDamage(30);
            game.Step(Tick, default(ArenaInput));
            var shield = new ArenaItem(9001, "Shield", ItemKind.Shield, shield: 15);
            game.Inventory.Equipment[2] = shield;
            game.Inventory.Equipment[3] = new ArenaItem(9002, "Grenade", ItemKind.Grenade);
            game.Step(Tick, new ArenaInput { UseB = true, FireA = true, HasAim = true, AimPoint = Vector2.up });
            Assert.That(game.Grenades.Count, Is.EqualTo(1));
            Assert.That(game.WeaponCooldowns[0], Is.GreaterThan(0f));
            float delay = game.Inventory.ShieldDelayRemaining;
            float elapsed = game.Elapsed;
            Assert.That(game.TryAdvanceFloor(), Is.True);
            Assert.That(game.Grenades, Is.Empty);
            Assert.That(game.Effects, Is.Empty);
            Assert.That(game.Inventory.Chest, Is.Empty);
            game.Step(ArenaSimulation.TransitionDuration, new ArenaInput { UseA = true, FireA = true, Move = Vector2.up });
            Assert.That(game.Inventory.Health, Is.EqualTo(80));
            Assert.That(game.Inventory.MaxHealth, Is.EqualTo(110));
            Assert.That(game.Inventory.Equipment[2], Is.SameAs(shield));
            Assert.That(shield.Shield, Is.EqualTo(15));
            Assert.That(game.Inventory.Equipment[3], Is.Null, "Consumed grenade is not refunded when its projectile is cleared.");
            Assert.That(game.Inventory.ShieldDelayRemaining, Is.EqualTo(delay));
            Assert.That(game.Elapsed, Is.EqualTo(elapsed));
            Assert.That(game.WeaponCooldowns, Is.All.Zero);
            Assert.That(game.PlayerPosition, Is.EqualTo(ArenaSimulation.EntrancePosition));
        }

        [Test]
        public void ClearRemovesHostileShotsAndGrenadesBeforeSafeStateCanBeDamaged()
        {
            var game = AtFloor(1);
            game.Projectiles.Add(new ArenaProjectile
            {
                Id = 9000, Position = Vector2.up * 8f, Direction = Vector2.down,
                Speed = 1f, DistanceRemaining = 20f, Damage = 10000, EnemyOwned = true
            });
            game.Inventory.Equipment[2] = new ArenaItem(9001, "Grenade", ItemKind.Grenade);
            game.Step(Tick, new ArenaInput { HasAim = true, AimPoint = Vector2.up, UseA = true });
            Assert.That(game.Grenades.Count, Is.EqualTo(1));
            EliminateRosterWithInput(game);
            Assert.That(game.Projectiles, Is.Empty);
            Assert.That(game.Grenades, Is.Empty);
            Assert.That(game.Effects, Is.Empty);
            int health = game.Inventory.Health;
            game.Step(10f, default(ArenaInput));
            Assert.That(game.Phase, Is.EqualTo(ArenaPhase.Cleared));
            Assert.That(game.Inventory.Health, Is.EqualTo(health));
        }

        [Test]
        public void MovementIsNormalizedWallBoundAndMaintainsLastValidAim()
        {
            var game = Started();
            game.PlayerPosition = Vector2.zero;
            game.Step(1f, new ArenaInput { Move = Vector2.one, HasAim = true, AimPoint = Vector2.right * 8f });
            Assert.That(game.PlayerPosition.magnitude, Is.EqualTo(5f).Within(0.0001f));
            Vector2 aim = game.AimDirection;
            game.Step(Tick, new ArenaInput { HasAim = true, AimPoint = game.PlayerPosition });
            Assert.That(game.AimDirection, Is.EqualTo(aim));
            game.Step(10f, new ArenaInput { Move = Vector2.right });
            Assert.That(game.PlayerPosition.x, Is.EqualTo(ArenaSimulation.ArenaHalfExtent - ArenaSimulation.PlayerRadius));
        }

        [Test]
        public void DeathSnapshotStaysFixedAndRetryResetsRunWhileMenuClearsIt()
        {
            var game = AtFloor(1);
            var enemy = KeepOneEnemy(game);
            Freeze(enemy, game.PlayerPosition + Vector2.up, 100);
            enemy.Damage = 1000;
            enemy.AttackCooldown = 0f;
            game.Step(Tick, default(ArenaInput));
            ArenaResult result = game.Result;
            Assert.That(result.EquipmentNames[0], Is.EqualTo("Blade"));
            float elapsed = result.Elapsed;
            game.StartRun();
            Assert.That(game.Result, Is.Null);
            Assert.That(game.Inventory.Health, Is.EqualTo(100));
            Assert.That(game.Inventory.AttackMultiplier, Is.EqualTo(1f));
            Assert.That(game.Floor, Is.Zero);
            Assert.That(game.EnemiesDefeated, Is.Zero);
            Assert.That(game.Elapsed, Is.Zero);
            Assert.That(result.Floor, Is.EqualTo(1));
            Assert.That(result.Elapsed, Is.EqualTo(elapsed));
            Assert.That(result.EquipmentNames[0], Is.EqualTo("Blade"));
            game.ReturnToMenu();
            Assert.That(game.Phase, Is.EqualTo(ArenaPhase.Opening));
            Assert.That(game.Inventory.Equipment.All(item => item == null), Is.True);
            Assert.That(game.Inventory.Backpack.All(item => item == null), Is.True);
            Assert.That(game.Inventory.Chest, Is.Empty);
            Assert.That(game.Enemies, Is.Empty);
            Assert.That(game.Result, Is.Null);
        }

        private static ArenaSimulation Started()
        {
            var game = new ArenaSimulation();
            game.StartRun();
            return game;
        }

        private static void EnterNext(ArenaSimulation game)
        {
            Assert.That(game.TryAdvanceFloor(), Is.True);
            game.Step(ArenaSimulation.TransitionDuration, default(ArenaInput));
            Assert.That(game.Phase, Is.EqualTo(ArenaPhase.Combat));
        }

        private static ArenaSimulation AtFloor(int floor)
        {
            var game = Started();
            for (int i = 1; i <= floor; i++)
            {
                EnterNext(game);
                if (i != floor) EliminateRosterWithInput(game);
            }
            // Fixtures accelerate earlier floors but the test starts with the real basic weapon.
            game.Inventory.Equipment[0] = new ArenaItem(10000, "Blade", ItemKind.Blade, 20, 0.5f);
            return game;
        }

        private static ArenaEnemy KeepOneEnemy(ArenaSimulation game)
        {
            ArenaEnemy enemy = game.Enemies[0];
            game.Enemies.RemoveRange(1, game.Enemies.Count - 1);
            return enemy;
        }

        private static void Freeze(ArenaEnemy enemy, Vector2 position, int health)
        {
            enemy.Position = position;
            enemy.Health = health;
            enemy.Speed = 0f;
            enemy.AttackCooldown = 1000f;
        }

        // Test setup moves the entire roster into a high-damage blade cone. Elimination,
        // counters, death priority and the actual transition still run through public input.
        private static void EliminateRosterWithInput(ArenaSimulation game)
        {
            game.Inventory.Equipment[0] = new ArenaItem(10000, "Fixture Blade", ItemKind.Blade, 1000000, 0.5f);
            game.WeaponCooldowns[0] = 0f;
            foreach (var enemy in game.Enemies)
                Freeze(enemy, game.PlayerPosition + Vector2.up, enemy.Health);
            game.Step(Tick, new ArenaInput { FireA = true, HasAim = true, AimPoint = game.PlayerPosition + Vector2.up * 3f });
        }
    }
}
