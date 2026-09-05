using System;
using System.Linq;
using UnityEngine;

namespace UnityOnlyArena
{
    // Detached diagnostic projections, not a save-state or an alternate simulation.
    [Serializable]
    public sealed class ArenaItemState
    {
        public int id, kind, damage, shield;
        public string name;
        public float interval, shieldFraction;

        internal static ArenaItemState Capture(ArenaItem item) => item == null
            ? new ArenaItemState { id = 0, name = "" }
            : new ArenaItemState { id = item.Id, kind = (int)item.Kind, name = item.Name,
                damage = item.Damage, interval = item.Interval, shield = item.Shield,
                shieldFraction = item.ShieldFraction };
    }

    [Serializable]
    public sealed class ArenaInventoryState
    {
        public int health, maxHealth, nextItemId, attackUpgrades;
        public float attackMultiplier, shieldDelayRemaining;
        public bool damagedThisUpdate;
        public ArenaItemState[] backpack, equipment, chest;
    }

    [Serializable]
    public sealed class ArenaResultState
    {
        public bool present;
        public int floor, enemiesDefeated, bossesDefeated, floorsCleared, maxHealth;
        public float elapsed, attackMultiplier;
        public string[] equipmentNames = Array.Empty<string>();
    }

    [Serializable]
    public sealed class ArenaReferenceState
    {
        public long tick, simulationSteps;
        public int phase, floor, enemiesDefeated, bossesDefeated, floorsCleared, nextEntityId;
        public string overlay;
        public float elapsed, transitionRemaining;
        public bool portalArmed, portalAvailable, chestAvailable;
        public Vector2 playerPosition, aimDirection;
        public float[] weaponCooldowns;
        public ArenaInventoryState inventory;
        public ArenaEnemy[] enemies;
        public ArenaProjectile[] projectiles;
        public ArenaGrenade[] grenades;
        public ArenaEffect[] effects;
        public ArenaResultState result;
    }

    public sealed partial class ArenaInventory
    {
        /// <summary>Copies all rule state without retaining references to owned items.</summary>
        public ArenaInventoryState CaptureReferenceState() => new ArenaInventoryState
        {
            health = Health, maxHealth = MaxHealth, attackMultiplier = AttackMultiplier,
            shieldDelayRemaining = ShieldDelayRemaining, nextItemId = nextItemId,
            attackUpgrades = attackUpgrades, damagedThisUpdate = damagedThisUpdate,
            backpack = Backpack.Select(ArenaItemState.Capture).ToArray(),
            equipment = Equipment.Select(ArenaItemState.Capture).ToArray(),
            chest = Chest.Select(ArenaItemState.Capture).ToArray(),
        };
    }

    public sealed partial class ArenaSimulation
    {
        /// <summary>Copies post-step state, including timers and identities that affect future ticks.</summary>
        public ArenaReferenceState CaptureReferenceState(long tick, long simulationSteps, string overlay)
        {
            var result = new ArenaResultState();
            if (Result != null)
                result = new ArenaResultState { present = true, floor = Result.Floor,
                    elapsed = Result.Elapsed, enemiesDefeated = Result.EnemiesDefeated,
                    bossesDefeated = Result.BossesDefeated, floorsCleared = Result.FloorsCleared,
                    maxHealth = Result.MaxHealth, attackMultiplier = Result.AttackMultiplier,
                    equipmentNames = Result.EquipmentNames.ToArray() };
            return new ArenaReferenceState
            {
                tick = tick, simulationSteps = simulationSteps, overlay = overlay,
                phase = (int)Phase, floor = Floor, elapsed = Elapsed,
                enemiesDefeated = EnemiesDefeated, bossesDefeated = BossesDefeated,
                floorsCleared = FloorsCleared, nextEntityId = nextId,
                transitionRemaining = transitionRemaining, portalArmed = portalArmed,
                portalAvailable = PortalAvailable, chestAvailable = ChestAvailable,
                playerPosition = PlayerPosition, aimDirection = AimDirection,
                weaponCooldowns = (float[])WeaponCooldowns.Clone(),
                inventory = Inventory.CaptureReferenceState(),
                enemies = Enemies.Select(CopyReferenceValue).ToArray(),
                projectiles = Projectiles.Select(CopyReferenceValue).ToArray(),
                grenades = Grenades.Select(CopyReferenceValue).ToArray(),
                effects = Effects.Select(CopyReferenceValue).ToArray(), result = result,
            };
        }

        private static T CopyReferenceValue<T>(T value) => JsonUtility.FromJson<T>(JsonUtility.ToJson(value));
    }
}
