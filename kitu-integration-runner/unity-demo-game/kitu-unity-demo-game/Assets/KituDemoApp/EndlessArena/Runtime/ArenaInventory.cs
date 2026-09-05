using System;
using System.Collections.Generic;

namespace UnityOnlyArena
{
    public enum ItemKind
    {
        Blade, Shooter, HeavyShooter, Medkit, Grenade, Shield, HealthUpgrade, AttackUpgrade
    }

    public enum EquipmentSlot
    {
        WeaponA = 0, WeaponB = 1, ItemA = 2, ItemB = 3
    }

    /// <summary>One owned item instance. Moving it never recreates its shield charge.</summary>
    public sealed class ArenaItem
    {
        public int Id { get; }
        public string Name { get; }
        public ItemKind Kind { get; }
        public int Damage { get; }
        public float Interval { get; }
        public int Shield { get; internal set; }
        public float ShieldFraction { get; internal set; }
        public bool IsWeapon => Kind == ItemKind.Blade || Kind == ItemKind.Shooter || Kind == ItemKind.HeavyShooter;
        public bool IsUpgrade => Kind == ItemKind.HealthUpgrade || Kind == ItemKind.AttackUpgrade;

        public ArenaItem(int id, string name, ItemKind kind, int damage = 0, float interval = 0f, int shield = 0)
        {
            Id = id;
            Name = name;
            Kind = kind;
            Damage = damage;
            Interval = interval;
            Shield = Math.Max(0, shield);
        }
    }

    /// <summary>
    /// Unity-only run inventory and health rules. The simulation owns phase checks and
    /// advances this clock once, after all damage in a running update is resolved.
    /// </summary>
    public sealed class ArenaInventory
    {
        public const int BackpackCapacity = 3;
        public const float ShieldRecoveryDelay = 3f;
        public const float ShieldRecoveryRate = 0.1f;

        public ArenaItem[] Backpack { get; } = new ArenaItem[BackpackCapacity];
        public ArenaItem[] Equipment { get; } = new ArenaItem[4];
        public List<ArenaItem> Chest { get; } = new List<ArenaItem>();
        public int Health { get; private set; }
        public int MaxHealth { get; private set; }
        public float AttackMultiplier { get; private set; }
        public float ShieldDelayRemaining { get; private set; }
        public string LastMessage { get; private set; }

        private int nextItemId;
        private bool damagedThisUpdate;
        private int attackUpgrades;

        public ArenaInventory()
        {
            Reset();
        }

        public void Reset()
        {
            Array.Clear(Backpack, 0, Backpack.Length);
            Array.Clear(Equipment, 0, Equipment.Length);
            Chest.Clear();
            nextItemId = 0;
            Health = MaxHealth = 100;
            attackUpgrades = 0;
            AttackMultiplier = 1f;
            ShieldDelayRemaining = 0f;
            damagedThisUpdate = false;
            Equipment[(int)EquipmentSlot.WeaponA] = NewItem("Blade", ItemKind.Blade, 20, 0.5f);
            LastMessage = "Choose equipment, then enter the portal.";
        }

        /// <summary>Called only when a new preparation/boss reward chest is created.</summary>
        public void CreateChest(int floor)
        {
            Chest.Clear();
            bool bossReward = floor >= 5;
            Chest.Add(NewItem("Quick Blade", ItemKind.Blade, bossReward ? 24 : 20, 0.5f));
            Chest.Add(NewItem("Power Blade", ItemKind.Blade, bossReward ? 36 : 30, 0.8f));
            Chest.Add(NewItem("Quick Shooter", ItemKind.Shooter, bossReward ? 15 : 12, 0.25f));
            Chest.Add(NewItem("Power Shooter", ItemKind.Shooter, bossReward ? 24 : 20, 0.45f));
            Chest.Add(NewItem("Quick Heavy Shooter", ItemKind.HeavyShooter, bossReward ? 60 : 50, 1f));
            Chest.Add(NewItem("Power Heavy Shooter", ItemKind.HeavyShooter, bossReward ? 90 : 75, 1.5f));
            Chest.Add(NewItem("Medkit", ItemKind.Medkit));
            Chest.Add(NewItem("Grenade", ItemKind.Grenade, 100));
            Chest.Add(NewItem("Shield", ItemKind.Shield, shield: MaxHealth));
            Chest.Add(NewItem("Health Upgrade (+10 HP)", ItemKind.HealthUpgrade));
            Chest.Add(NewItem("Attack Upgrade (+5%)", ItemKind.AttackUpgrade));
            LastMessage = "Select a backpack slot to take or swap an item.";
        }

        public bool TakeChest(int chestIndex, int backpackIndex)
        {
            if (Health <= 0 || !ValidBackpack(backpackIndex) || chestIndex < 0 || chestIndex >= Chest.Count || Chest[chestIndex] == null)
                return Reject("Select an available chest item and backpack slot.");

            ArenaItem incoming = Chest[chestIndex];
            ArenaItem outgoing = Backpack[backpackIndex];
            if (outgoing == null)
                Chest.RemoveAt(chestIndex);
            else
                Chest[chestIndex] = outgoing;
            Backpack[backpackIndex] = incoming;
            LastMessage = outgoing == null ? "Took " + incoming.Name + "." : "Swapped " + outgoing.Name + " for " + incoming.Name + ".";
            return true;
        }

        public bool Equip(int backpackIndex, EquipmentSlot slot)
        {
            if (Health <= 0 || !ValidBackpack(backpackIndex) || !ValidSlot(slot))
                return Reject("Select an available backpack item and equipment slot.");
            ArenaItem item = Backpack[backpackIndex];
            if (item == null || item.IsUpgrade || item.IsWeapon != ((int)slot < 2))
                return Reject("Weapons fit weapon slots; medkits, grenades and shields fit item slots.");

            Backpack[backpackIndex] = Equipment[(int)slot];
            Equipment[(int)slot] = item;
            LastMessage = "Equipped " + item.Name + ".";
            return true;
        }

        public bool Unequip(EquipmentSlot slot)
        {
            if (Health <= 0 || !ValidSlot(slot) || Equipment[(int)slot] == null)
                return Reject("There is no equipment in that slot.");
            int emptySlot = Array.FindIndex(Backpack, item => item == null);
            if (emptySlot < 0)
                return Reject("Backpack is full. Swap or discard an item first.");

            Backpack[emptySlot] = Equipment[(int)slot];
            Equipment[(int)slot] = null;
            LastMessage = "Moved equipment to backpack.";
            return true;
        }

        public bool Discard(int backpackIndex)
        {
            if (Health <= 0 || !ValidBackpack(backpackIndex) || Backpack[backpackIndex] == null)
                return Reject("Select a backpack item to discard.");
            LastMessage = "Discarded " + Backpack[backpackIndex].Name + ".";
            Backpack[backpackIndex] = null;
            return true;
        }

        public bool UseUpgrade(int backpackIndex)
        {
            if (Health <= 0 || !ValidBackpack(backpackIndex) || Backpack[backpackIndex] == null || !Backpack[backpackIndex].IsUpgrade)
                return Reject("Select an upgrade in the backpack.");
            ArenaItem item = Backpack[backpackIndex];
            if (item.Kind == ItemKind.HealthUpgrade)
            {
                MaxHealth += 10;
                Health = Math.Min(MaxHealth, Health + 10);
                // Shield capacities follow MaxHealth; neither their charge nor fractions change.
            }
            else
            {
                attackUpgrades++;
                AttackMultiplier = 1f + attackUpgrades * 0.05f;
            }
            Backpack[backpackIndex] = null;
            LastMessage = "Used " + item.Name + ".";
            return true;
        }

        /// <summary>The caller must validate grenade aiming before invoking this method.</summary>
        public bool UseItem(EquipmentSlot slot, out ArenaItem used)
        {
            used = null;
            if (Health <= 0 || (slot != EquipmentSlot.ItemA && slot != EquipmentSlot.ItemB))
                return Reject("This item cannot be used now.");
            ArenaItem item = Equipment[(int)slot];
            if (item == null)
                return false;
            if (item.Kind == ItemKind.Shield)
                return Reject("Shield protects and recovers automatically.");
            if (item.Kind == ItemKind.Medkit)
            {
                if (Health >= MaxHealth)
                    return Reject("HP is already full; medkit was kept.");
                Health = MaxHealth;
            }
            else if (item.Kind != ItemKind.Grenade)
                return Reject("This is not a usable item.");

            Equipment[(int)slot] = null;
            used = item;
            LastMessage = "Used " + item.Name + ".";
            return true;
        }

        public void ApplyDamage(int damage)
        {
            if (damage <= 0 || Health <= 0)
                return;
            ShieldDelayRemaining = ShieldRecoveryDelay;
            damagedThisUpdate = true;
            for (int slot = (int)EquipmentSlot.ItemA; slot <= (int)EquipmentSlot.ItemB; slot++)
            {
                ArenaItem item = Equipment[slot];
                if (item == null || item.Kind != ItemKind.Shield)
                    continue;
                item.Shield = Math.Min(MaxHealth, item.Shield);
                int absorbed = Math.Min(item.Shield, damage);
                item.Shield -= absorbed;
                damage -= absorbed;
            }
            Health = Math.Max(0, Health - damage);
        }

        public void HealFully()
        {
            if (Health > 0)
                Health = MaxHealth;
        }

        public void AdvanceTime(float dt)
        {
            // Damage wins for the entire update, even if a large dt crosses the delay.
            if (damagedThisUpdate)
            {
                damagedThisUpdate = false;
                return;
            }
            if (dt <= 0f || float.IsNaN(dt) || float.IsInfinity(dt) || Health <= 0)
                return;
            float recoveryTime = Math.Max(0f, dt - ShieldDelayRemaining);
            ShieldDelayRemaining = Math.Max(0f, ShieldDelayRemaining - dt);
            if (recoveryTime <= 0f)
                return;

            for (int slot = (int)EquipmentSlot.ItemA; slot <= (int)EquipmentSlot.ItemB; slot++)
            {
                ArenaItem item = Equipment[slot];
                if (item == null || item.Kind != ItemKind.Shield)
                    continue;
                float recovered = item.ShieldFraction + recoveryTime * MaxHealth * ShieldRecoveryRate;
                int wholePoints = (int)Math.Floor(recovered + 0.00001f);
                item.Shield = Math.Min(MaxHealth, item.Shield + wholePoints);
                item.ShieldFraction = item.Shield >= MaxHealth ? 0f : Math.Max(0f, recovered - wholePoints);
            }
        }

        private ArenaItem NewItem(string name, ItemKind kind, int damage = 0, float interval = 0f, int shield = 0)
        {
            return new ArenaItem(++nextItemId, name, kind, damage, interval, shield);
        }

        private bool Reject(string message)
        {
            LastMessage = message;
            return false;
        }

        private static bool ValidSlot(EquipmentSlot slot) => (int)slot >= 0 && (int)slot < 4;
        private static bool ValidBackpack(int index) => index >= 0 && index < BackpackCapacity;
    }
}
