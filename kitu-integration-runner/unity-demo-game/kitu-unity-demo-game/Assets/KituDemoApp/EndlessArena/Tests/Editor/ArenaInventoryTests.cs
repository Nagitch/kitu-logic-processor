using System;
using System.Collections.Generic;
using System.Linq;
using NUnit.Framework;
using UnityOnlyArena;

public sealed class ArenaInventoryTests
{
    [Test]
    public void ResetRemovesAllRunStateAndRestoresOnlyTheStarterBlade()
    {
        var inventory = ReadyInventory();
        TakeKind(inventory, ItemKind.HealthUpgrade);
        Assert.That(inventory.UseUpgrade(0), Is.True);
        TakeKind(inventory, ItemKind.AttackUpgrade);
        Assert.That(inventory.UseUpgrade(0), Is.True);
        EquipKind(inventory, ItemKind.Shield, EquipmentSlot.ItemA);
        inventory.ApplyDamage(125);

        inventory.Reset();

        Assert.That(inventory.Health, Is.EqualTo(100));
        Assert.That(inventory.MaxHealth, Is.EqualTo(100));
        Assert.That(inventory.AttackMultiplier, Is.EqualTo(1f));
        Assert.That(inventory.ShieldDelayRemaining, Is.Zero);
        Assert.That(inventory.Chest, Is.Empty);
        Assert.That(inventory.Backpack, Is.All.Null);
        Assert.That(inventory.Equipment[0].Kind, Is.EqualTo(ItemKind.Blade));
        Assert.That(inventory.Equipment[0].Damage, Is.EqualTo(20));
        Assert.That(inventory.Equipment[0].Interval, Is.EqualTo(0.5f));
        Assert.That(inventory.Equipment.Skip(1), Is.All.Null);
    }

    [Test]
    public void ChestHasSixDistinctWeaponCandidatesAndFiveItemUpgradeCandidates()
    {
        var inventory = ReadyInventory();
        Assert.That(inventory.Chest.Count, Is.EqualTo(11));
        Assert.That(inventory.Chest.Select(item => item.Id).Distinct().Count(), Is.EqualTo(11));
        Assert.That(inventory.Chest.Count(item => item.IsWeapon), Is.EqualTo(6));
        foreach (ItemKind kind in new[] { ItemKind.Blade, ItemKind.Shooter, ItemKind.HeavyShooter })
        {
            var candidates = inventory.Chest.Where(item => item.Kind == kind).ToArray();
            Assert.That(candidates.Length, Is.EqualTo(2));
            Assert.That(candidates[0].Damage, Is.LessThan(candidates[1].Damage));
            Assert.That(candidates[0].Interval, Is.LessThan(candidates[1].Interval));
        }
        Assert.That(inventory.Chest.Count(item => item.IsUpgrade), Is.EqualTo(2));
        Assert.That(inventory.Chest.Single(item => item.Kind == ItemKind.Shield).Shield, Is.EqualTo(100));
        Assert.That(inventory.Chest.Single(item => item.Kind == ItemKind.Grenade).Damage, Is.EqualTo(100));

        inventory.CreateChest(5);
        int[] firstBossDamage = inventory.Chest.Select(item => item.Damage).ToArray();
        inventory.CreateChest(100);
        Assert.That(inventory.Chest.Select(item => item.Damage), Is.EqualTo(firstBossDamage), "Boss reward table must repeat.");
    }

    [Test]
    public void TakingSwappingAndEquippingConserveItemInstancesEvenWhenBackpackIsFull()
    {
        var inventory = ReadyInventory();
        var original = AllItems(inventory).ToArray();
        Assert.That(inventory.TakeChest(0, 0), Is.True);
        Assert.That(inventory.TakeChest(0, 1), Is.True);
        Assert.That(inventory.TakeChest(0, 2), Is.True);
        var outgoing = inventory.Backpack[1];
        var incoming = inventory.Chest[0];
        Assert.That(inventory.TakeChest(0, 1), Is.True);
        Assert.That(inventory.Backpack[1], Is.SameAs(incoming));
        Assert.That(inventory.Chest[0], Is.SameAs(outgoing));

        var starter = inventory.Equipment[0];
        Assert.That(inventory.Equip(1, EquipmentSlot.WeaponA), Is.True);
        Assert.That(inventory.Equipment[0], Is.SameAs(incoming));
        Assert.That(inventory.Backpack[1], Is.SameAs(starter));
        Assert.That(AllItems(inventory), Is.EquivalentTo(original));
        Assert.That(AllItems(inventory).Distinct().Count(), Is.EqualTo(original.Length));
    }

    [Test]
    public void InvalidOperationsAreAtomicAndUnequipNeedsAnEmptyBackpackSlot()
    {
        var inventory = ReadyInventory();
        TakeKind(inventory, ItemKind.Medkit, 0);
        TakeKind(inventory, ItemKind.HealthUpgrade, 1);
        TakeKind(inventory, ItemKind.Shooter, 2);
        var backpack = (ArenaItem[])inventory.Backpack.Clone();
        var equipment = (ArenaItem[])inventory.Equipment.Clone();
        var chest = inventory.Chest.ToArray();

        Assert.That(inventory.Equip(0, EquipmentSlot.WeaponA), Is.False);
        Assert.That(inventory.Equip(1, EquipmentSlot.ItemA), Is.False);
        Assert.That(inventory.Equip(2, EquipmentSlot.ItemA), Is.False);
        Assert.That(inventory.Equip(-1, EquipmentSlot.ItemA), Is.False);
        Assert.That(inventory.Equip(0, (EquipmentSlot)99), Is.False);
        Assert.That(inventory.Unequip(EquipmentSlot.WeaponA), Is.False);
        Assert.That(inventory.TakeChest(-1, 0), Is.False);
        Assert.That(inventory.TakeChest(0, 3), Is.False);
        Assert.That(inventory.UseUpgrade(2), Is.False);
        Assert.That(inventory.Discard(3), Is.False);
        Assert.That(inventory.Backpack, Is.EqualTo(backpack));
        Assert.That(inventory.Equipment, Is.EqualTo(equipment));
        Assert.That(inventory.Chest, Is.EqualTo(chest));

        Assert.That(inventory.Discard(1), Is.True);
        Assert.That(inventory.Unequip(EquipmentSlot.WeaponA), Is.True);
        Assert.That(inventory.Backpack[1], Is.SameAs(equipment[0]));
        Assert.That(inventory.Equipment[0], Is.Null);
        Assert.That(inventory.Discard(1), Is.True);
        Assert.That(inventory.Discard(1), Is.False);
    }

    [Test]
    public void UpgradesConsumeOnceAndHealthUpgradeDoesNotRefillAnExistingShield()
    {
        var inventory = ReadyInventory();
        var shield = EquipKind(inventory, ItemKind.Shield, EquipmentSlot.ItemA);
        inventory.ApplyDamage(125);
        TakeKind(inventory, ItemKind.HealthUpgrade);
        Assert.That(inventory.UseUpgrade(0), Is.True);
        Assert.That(inventory.UseUpgrade(0), Is.False);
        Assert.That(inventory.MaxHealth, Is.EqualTo(110));
        Assert.That(inventory.Health, Is.EqualTo(85));
        Assert.That(shield.Shield, Is.Zero);
        TakeKind(inventory, ItemKind.AttackUpgrade);
        Assert.That(inventory.UseUpgrade(0), Is.True);
        Assert.That(inventory.UseUpgrade(0), Is.False);
        Assert.That(inventory.AttackMultiplier, Is.EqualTo(1.05f).Within(0.00001f));

        inventory.CreateChest(5);
        Assert.That(inventory.Chest.Single(item => item.Kind == ItemKind.Shield).Shield, Is.EqualTo(110));
        Assert.That(shield.Shield, Is.Zero);
    }

    [Test]
    public void FullHealthMedkitIsKeptAndUsedMedkitOnlyRefillsHp()
    {
        var inventory = ReadyInventory();
        var medkit = EquipKind(inventory, ItemKind.Medkit, EquipmentSlot.ItemA);
        var shield = EquipKind(inventory, ItemKind.Shield, EquipmentSlot.ItemB);
        Assert.That(inventory.UseItem(EquipmentSlot.ItemA, out ArenaItem unused), Is.False);
        Assert.That(unused, Is.Null);
        Assert.That(inventory.Equipment[2], Is.SameAs(medkit));
        inventory.ApplyDamage(130);
        Assert.That(inventory.UseItem(EquipmentSlot.ItemA, out ArenaItem used), Is.True);
        Assert.That(used, Is.SameAs(medkit));
        Assert.That(inventory.Equipment[2], Is.Null);
        Assert.That(inventory.Health, Is.EqualTo(100));
        Assert.That(shield.Shield, Is.Zero);
        Assert.That(inventory.ShieldDelayRemaining, Is.EqualTo(3f));
    }

    [Test]
    public void UsingMedkitAThenBConsumesOnlyAWhenAHealsToFull()
    {
        var inventory = ReadyInventory();
        EquipKind(inventory, ItemKind.Medkit, EquipmentSlot.ItemA);
        inventory.CreateChest(5);
        var medkitB = EquipKind(inventory, ItemKind.Medkit, EquipmentSlot.ItemB);
        inventory.ApplyDamage(15);
        Assert.That(inventory.UseItem(EquipmentSlot.ItemA, out _), Is.True);
        Assert.That(inventory.UseItem(EquipmentSlot.ItemB, out _), Is.False);
        Assert.That(inventory.Equipment[2], Is.Null);
        Assert.That(inventory.Equipment[3], Is.SameAs(medkitB));
    }

    [Test]
    public void GrenadeIsConsumedImmediatelyAndBackpackDoesNotAutoRefillEquipment()
    {
        var inventory = ReadyInventory();
        var grenade = EquipKind(inventory, ItemKind.Grenade, EquipmentSlot.ItemB);
        inventory.CreateChest(5);
        TakeKind(inventory, ItemKind.Grenade);
        var spare = inventory.Backpack[0];
        Assert.That(inventory.UseItem(EquipmentSlot.ItemB, out ArenaItem used), Is.True);
        Assert.That(used, Is.SameAs(grenade));
        Assert.That(inventory.Equipment[3], Is.Null);
        Assert.That(inventory.Backpack[0], Is.SameAs(spare));
        Assert.That(inventory.UseItem(EquipmentSlot.ItemB, out _), Is.False);
        Assert.That(inventory.UseItem(EquipmentSlot.WeaponA, out _), Is.False);
    }

    [Test]
    public void ShieldsAbsorbInSlotOrderAndRemainEquippedWhenEmpty()
    {
        var inventory = ReadyInventory();
        var shieldA = EquipKind(inventory, ItemKind.Shield, EquipmentSlot.ItemA);
        inventory.CreateChest(5);
        var shieldB = EquipKind(inventory, ItemKind.Shield, EquipmentSlot.ItemB);
        Assert.That(inventory.UseItem(EquipmentSlot.ItemA, out _), Is.False);
        inventory.ApplyDamage(130);
        Assert.That(shieldA.Shield, Is.Zero);
        Assert.That(shieldB.Shield, Is.EqualTo(70));
        Assert.That(inventory.Health, Is.EqualTo(100));
        inventory.ApplyDamage(100);
        Assert.That(shieldB.Shield, Is.Zero);
        Assert.That(inventory.Health, Is.EqualTo(70));
        Assert.That(inventory.Equipment[2], Is.SameAs(shieldA));
        Assert.That(inventory.Equipment[3], Is.SameAs(shieldB));
    }

    [Test]
    public void DamageUpdateCannotRegenerateAndRecoveryStartsAfterThreeSeconds()
    {
        var inventory = ReadyInventory();
        var shield = EquipKind(inventory, ItemKind.Shield, EquipmentSlot.ItemA);
        inventory.ApplyDamage(35);
        inventory.AdvanceTime(100f);
        Assert.That(shield.Shield, Is.EqualTo(65));
        Assert.That(inventory.ShieldDelayRemaining, Is.EqualTo(3f));
        inventory.AdvanceTime(2.5f);
        Assert.That(shield.Shield, Is.EqualTo(65));
        inventory.AdvanceTime(0.5f);
        Assert.That(shield.Shield, Is.EqualTo(65));
        Assert.That(inventory.ShieldDelayRemaining, Is.Zero);
        inventory.AdvanceTime(1f);
        Assert.That(shield.Shield, Is.EqualTo(75));
        inventory.AdvanceTime(10f);
        Assert.That(shield.Shield, Is.EqualTo(100));
        Assert.That(shield.ShieldFraction, Is.Zero);
    }

    [Test]
    public void FractionalRecoveryAccumulatesAcrossSmallUpdatesAndUsesNewCapacity()
    {
        var inventory = ReadyInventory();
        var shield = EquipKind(inventory, ItemKind.Shield, EquipmentSlot.ItemA);
        inventory.ApplyDamage(100);
        inventory.AdvanceTime(0f);
        inventory.AdvanceTime(3f);
        for (int i = 0; i < 100; i++)
            inventory.AdvanceTime(0.01f);
        Assert.That(shield.Shield, Is.EqualTo(10));
        TakeKind(inventory, ItemKind.HealthUpgrade);
        inventory.UseUpgrade(0);
        Assert.That(shield.Shield, Is.EqualTo(10));
        inventory.AdvanceTime(1f);
        Assert.That(shield.Shield, Is.EqualTo(21));
        inventory.AdvanceTime(30f);
        Assert.That(shield.Shield, Is.EqualTo(110));
    }

    [Test]
    public void ShieldChargeAndFractionSurviveSwapsButOnlyEquipmentRegenerates()
    {
        var inventory = ReadyInventory();
        var shield = EquipKind(inventory, ItemKind.Shield, EquipmentSlot.ItemA);
        inventory.ApplyDamage(50);
        inventory.AdvanceTime(0f);
        inventory.AdvanceTime(3.05f);
        Assert.That(shield.Shield, Is.EqualTo(50));
        Assert.That(shield.ShieldFraction, Is.EqualTo(0.5f).Within(0.00001f));
        Assert.That(inventory.Unequip(EquipmentSlot.ItemA), Is.True);
        inventory.AdvanceTime(5f);
        Assert.That(shield.Shield, Is.EqualTo(50));
        Assert.That(inventory.TakeChest(0, 0), Is.True);
        Assert.That(inventory.Chest[0], Is.SameAs(shield));
        inventory.AdvanceTime(5f);
        Assert.That(shield.Shield, Is.EqualTo(50));
        Assert.That(inventory.TakeChest(0, 0), Is.True);
        Assert.That(inventory.Equip(0, EquipmentSlot.ItemB), Is.True);
        inventory.AdvanceTime(0.05f);
        Assert.That(shield.Shield, Is.EqualTo(51));
        Assert.That(shield.ShieldFraction, Is.Zero.Within(0.00001f));
    }

    [Test]
    public void DamageWithoutAShieldStillStartsSharedDelayAndChangingEquipmentKeepsIt()
    {
        var inventory = ReadyInventory();
        TakeKind(inventory, ItemKind.Shield);
        var shield = inventory.Backpack[0];
        inventory.ApplyDamage(10);
        Assert.That(inventory.Health, Is.EqualTo(90), "Backpack shields cannot absorb damage.");
        inventory.AdvanceTime(0f);
        inventory.AdvanceTime(1f);
        Assert.That(inventory.ShieldDelayRemaining, Is.EqualTo(2f));
        Assert.That(inventory.Equip(0, EquipmentSlot.ItemA), Is.True);
        Assert.That(inventory.ShieldDelayRemaining, Is.EqualTo(2f));
        inventory.ApplyDamage(10);
        inventory.AdvanceTime(0f);
        Assert.That(shield.Shield, Is.EqualTo(90));
        Assert.That(inventory.Unequip(EquipmentSlot.ItemA), Is.True);
        inventory.AdvanceTime(3f);
        Assert.That(inventory.ShieldDelayRemaining, Is.Zero);
        Assert.That(inventory.Equip(0, EquipmentSlot.ItemB), Is.True);
        inventory.AdvanceTime(1f);
        Assert.That(shield.Shield, Is.EqualTo(100));
    }

    [Test]
    public void HealingDoesNotTouchShieldOrDelayAndNonPositiveDamageDoesNothing()
    {
        var inventory = ReadyInventory();
        var shield = EquipKind(inventory, ItemKind.Shield, EquipmentSlot.ItemA);
        inventory.ApplyDamage(125);
        inventory.AdvanceTime(0f);
        inventory.AdvanceTime(1f);
        inventory.HealFully();
        inventory.ApplyDamage(0);
        inventory.ApplyDamage(-10);
        Assert.That(inventory.Health, Is.EqualTo(100));
        Assert.That(shield.Shield, Is.Zero);
        Assert.That(inventory.ShieldDelayRemaining, Is.EqualTo(2f));
        inventory.AdvanceTime(3f);
        Assert.That(shield.Shield, Is.EqualTo(10));
    }

    [Test]
    public void DeathPreventsRevivalConsumptionAndFurtherInventoryChanges()
    {
        var inventory = ReadyInventory();
        EquipKind(inventory, ItemKind.Medkit, EquipmentSlot.ItemA);
        TakeKind(inventory, ItemKind.HealthUpgrade);
        var beforeDeath = AllItems(inventory).ToArray();
        inventory.ApplyDamage(200);
        inventory.HealFully();
        inventory.AdvanceTime(100f);
        Assert.That(inventory.UseItem(EquipmentSlot.ItemA, out _), Is.False);
        Assert.That(inventory.UseUpgrade(0), Is.False);
        Assert.That(inventory.TakeChest(0, 1), Is.False);
        Assert.That(inventory.Equip(0, EquipmentSlot.ItemB), Is.False);
        Assert.That(inventory.Unequip(EquipmentSlot.ItemA), Is.False);
        Assert.That(inventory.Discard(0), Is.False);
        Assert.That(inventory.Health, Is.Zero);
        Assert.That(inventory.MaxHealth, Is.EqualTo(100));
        Assert.That(AllItems(inventory), Is.EquivalentTo(beforeDeath));
    }

    private static ArenaInventory ReadyInventory()
    {
        var inventory = new ArenaInventory();
        inventory.CreateChest(0);
        return inventory;
    }

    private static void TakeKind(ArenaInventory inventory, ItemKind kind, int backpackIndex = 0)
    {
        int index = inventory.Chest.FindIndex(item => item.Kind == kind);
        Assert.That(index, Is.GreaterThanOrEqualTo(0), "Test setup needs " + kind);
        Assert.That(inventory.TakeChest(index, backpackIndex), Is.True);
    }

    private static ArenaItem EquipKind(ArenaInventory inventory, ItemKind kind, EquipmentSlot slot)
    {
        TakeKind(inventory, kind);
        Assert.That(inventory.Equip(0, slot), Is.True);
        return inventory.Equipment[(int)slot];
    }

    private static IEnumerable<ArenaItem> AllItems(ArenaInventory inventory)
    {
        return inventory.Backpack.Concat(inventory.Equipment).Concat(inventory.Chest).Where(item => item != null);
    }
}
