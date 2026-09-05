using System;
using System.Linq;
using NUnit.Framework;
using UnityEngine;

namespace UnityOnlyArena.Tests
{
    public sealed class ArenaRunScenarioTests
    {
        [Test]
        public void StockChestLoadoutAndContinuousMovementCanClearElevenFloors()
        {
            var game = new ArenaSimulation();
            game.StartRun();
            int preparedFloor = -1;
            int observedFloor = -1;
            int observedClears = 0;
            bool mustExitPortal = false;
            const float dt = 1f / 60f;
            for (int tick = 0; tick < 60 * 60 * 20; tick++)
            {
                if (game.Floor != observedFloor)
                {
                    observedFloor = game.Floor;
                    Console.WriteLine("[stock run] entered " + game.Floor + "F; time=" + game.Elapsed.ToString("F2") +
                        "; HP=" + game.Inventory.Health + "/" + game.Inventory.MaxHealth + "; kills=" + game.EnemiesDefeated);
                }
                if (game.FloorsCleared != observedClears)
                {
                    observedClears = game.FloorsCleared;
                    mustExitPortal = Vector2.Distance(game.PlayerPosition, ArenaSimulation.PortalPosition) <= ArenaSimulation.PortalRadius;
                    Console.WriteLine("[stock run] cleared " + observedClears + "F; time=" + game.Elapsed.ToString("F2") +
                        "; HP=" + game.Inventory.Health + "/" + game.Inventory.MaxHealth + "; kills=" + game.EnemiesDefeated);
                }
                Assert.That(game.Phase, Is.Not.EqualTo(ArenaPhase.Results),
                    "Stock-loadout movement bot died on " + game.Floor + "F at " + game.Elapsed.ToString("F2") + "s.");
                if (game.FloorsCleared >= 11) break;

                var input = new ArenaInput();
                if (game.IsSafe)
                {
                    if (mustExitPortal)
                    {
                        input.Move = Vector2.down;
                        if (Vector2.Distance(game.PlayerPosition, ArenaSimulation.PortalPosition) > ArenaSimulation.PortalRadius + 0.25f)
                            mustExitPortal = false;
                    }
                    else if (game.ChestAvailable && preparedFloor != game.Floor)
                    {
                        Vector2 toChest = ArenaSimulation.ChestPosition - game.PlayerPosition;
                        if (toChest.magnitude > 1.5f) input.Move = toChest.normalized;
                        else
                        {
                            SelectStockChestLoadout(game);
                            preparedFloor = game.Floor;
                        }
                    }
                    else input.Move = (ArenaSimulation.PortalPosition - game.PlayerPosition).normalized;
                }
                else if (game.Phase == ArenaPhase.Combat)
                {
                    // Follow the arena circumference while aiming at the nearest live enemy.
                    // This drives only the same Move/Aim/Fire/Use inputs that the player uses.
                    float radius = game.PlayerPosition.magnitude;
                    Vector2 radial = radius > 0.01f ? game.PlayerPosition / radius : Vector2.down;
                    Vector2 tangent = new Vector2(-radial.y, radial.x);
                    input.Move = (tangent + radial * ((7.7f - radius) * 1.5f)).normalized;
                    ArenaEnemy target = game.Enemies.OrderBy(enemy => (enemy.Position - game.PlayerPosition).sqrMagnitude).First();
                    input.HasAim = true;
                    input.AimPoint = target.Position;
                    input.FireA = input.FireB = true;
                    input.UseB = game.Inventory.Health <= game.Inventory.MaxHealth / 2;
                }
                game.Step(dt, input);
            }
            Assert.That(game.Floor, Is.EqualTo(11));
            Assert.That(game.FloorsCleared, Is.EqualTo(11));
            Assert.That(game.Phase, Is.EqualTo(ArenaPhase.Cleared));
            Assert.That(game.BossesDefeated, Is.EqualTo(2));
            Assert.That(game.Inventory.MaxHealth, Is.EqualTo(130));
            Assert.That(game.Inventory.AttackMultiplier, Is.EqualTo(1.15f).Within(0.0001f));
            Assert.That(game.Inventory.Health, Is.GreaterThan(0));
            Console.WriteLine("[stock run] completed 11 floors; time=" + game.Elapsed.ToString("F2") +
                "; HP=" + game.Inventory.Health + "/" + game.Inventory.MaxHealth + "; kills=" + game.EnemiesDefeated + "; bosses=" + game.BossesDefeated);
        }

        private static void SelectStockChestLoadout(ArenaSimulation game)
        {
            Assert.That(game.IsSafe && game.ChestAvailable, Is.True);
            Assert.That(Vector2.Distance(game.PlayerPosition, ArenaSimulation.ChestPosition), Is.LessThanOrEqualTo(2f));
            EquipStockItem(game.Inventory, "Power Shooter", EquipmentSlot.WeaponA);
            EquipStockItem(game.Inventory, "Quick Shooter", EquipmentSlot.WeaponB);
            EquipStockItem(game.Inventory, "Shield", EquipmentSlot.ItemA);
            EquipStockItem(game.Inventory, "Medkit", EquipmentSlot.ItemB);
            foreach (ItemKind kind in new[] { ItemKind.HealthUpgrade, ItemKind.AttackUpgrade })
            {
                int chestIndex = game.Inventory.Chest.FindIndex(item => item.Kind == kind);
                int slot = Array.FindIndex(game.Inventory.Backpack, item => item == null);
                Assert.That(game.Inventory.TakeChest(chestIndex, slot), Is.True);
                Assert.That(game.Inventory.UseUpgrade(slot), Is.True);
            }
            Console.WriteLine("[stock run] chose " + game.Floor + "F chest; HP=" + game.Inventory.Health +
                "/" + game.Inventory.MaxHealth + "; attack=" + game.Inventory.AttackMultiplier.ToString("F2"));
        }

        private static void EquipStockItem(ArenaInventory inventory, string name, EquipmentSlot equipmentSlot)
        {
            int chestIndex = inventory.Chest.FindIndex(item => item.Name == name);
            int backpack = Array.FindIndex(inventory.Backpack, item => item == null);
            Assert.That(inventory.TakeChest(chestIndex, backpack), Is.True);
            Assert.That(inventory.Equip(backpack, equipmentSlot), Is.True);
            if (inventory.Backpack[backpack] != null) Assert.That(inventory.Discard(backpack), Is.True);
        }
    }
}
