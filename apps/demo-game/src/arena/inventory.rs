//! Arena-owned inventory rules, ported from the pinned C# reference.
//!
//! Item structs move between containers; IDs, charge and fractional recovery are
//! never recreated during a transfer. Network clients only see detached copies.

use serde::{Deserialize, Serialize};

/// One item instance; ID zero is the canonical empty-slot projection.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// Run-local instance identity.
    pub id: i32,
    /// C# ItemKind discriminant: blade, shooter, heavy, medkit, grenade, shield, HP upgrade, attack upgrade.
    pub kind: i32,
    /// Unscaled base damage.
    pub damage: i32,
    /// Remaining integral shield charge.
    pub shield: i32,
    /// Display name.
    pub name: String,
    /// Weapon interval in seconds.
    pub interval: f32,
    /// Fractional shield recovery retained across transfers.
    pub shield_fraction: f32,
}

impl Item {
    /// Whether the nonempty item fits a weapon slot.
    pub fn is_weapon(&self) -> bool {
        self.id > 0 && (0..=2).contains(&self.kind)
    }
    /// Whether the nonempty item is consumed by the upgrade action.
    pub fn is_upgrade(&self) -> bool {
        self.id > 0 && (self.kind == 6 || self.kind == 7)
    }
}

/// Inventory and shared health/shield clocks for one run.
///
/// # Examples
/// ```
/// use kitu_demo_game::arena::inventory::Inventory;
/// let mut inventory = Inventory::default();
/// inventory.create_chest(0);
/// assert!(inventory.take(0, 0));
/// assert!(inventory.equip(0, 1));
/// assert_eq!(inventory.equipment[1].name, "Quick Blade");
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    /// Current HP; zero prevents all further consumption, transfer or healing.
    pub health: i32,
    /// Current HP and shield capacity.
    pub max_health: i32,
    /// Last allocated item ID (matching the reference field name).
    pub next_item_id: i32,
    /// Number of consumed attack upgrades.
    pub attack_upgrades: i32,
    /// Derived attack multiplier.
    pub attack_multiplier: f32,
    /// Shared delay before equipped shields may recover.
    pub shield_delay_remaining: f32,
    /// Prevents recovery throughout an update that received damage.
    pub damaged_this_update: bool,
    /// Three stable backpack positions.
    pub backpack: [Item; 3],
    /// Weapon A/B and item A/B, in that order.
    pub equipment: [Item; 4],
    /// Ordered available chest items.
    pub chest: Vec<Item>,
}

impl Default for Inventory {
    fn default() -> Self {
        let mut value = Self {
            health: 100,
            max_health: 100,
            next_item_id: 0,
            attack_upgrades: 0,
            attack_multiplier: 1.0,
            shield_delay_remaining: 0.0,
            damaged_this_update: false,
            backpack: std::array::from_fn(|_| Item::default()),
            equipment: std::array::from_fn(|_| Item::default()),
            chest: Vec::new(),
        };
        value.equipment[0] = value.new_item("Blade", 0, 20, 0.5, 0);
        value
    }
}

impl Inventory {
    fn new_item(&mut self, name: &str, kind: i32, damage: i32, interval: f32, shield: i32) -> Item {
        self.next_item_id += 1;
        Item {
            id: self.next_item_id,
            name: name.into(),
            kind,
            damage,
            interval,
            shield: shield.max(0),
            shield_fraction: 0.0,
        }
    }

    /// Replaces the chest with the reference preparation/repeating boss reward table.
    pub fn create_chest(&mut self, floor: i32) {
        self.chest.clear();
        for (name, kind, initial, boss, interval) in [
            ("Quick Blade", 0, 20, 24, 0.5),
            ("Power Blade", 0, 30, 36, 0.8),
            ("Quick Shooter", 1, 12, 15, 0.25),
            ("Power Shooter", 1, 20, 24, 0.45),
            ("Quick Heavy Shooter", 2, 50, 60, 1.0),
            ("Power Heavy Shooter", 2, 75, 90, 1.5),
            ("Medkit", 3, 0, 0, 0.0),
            ("Grenade", 4, 100, 100, 0.0),
            ("Shield", 5, 0, 0, 0.0),
            ("Health Upgrade (+10 HP)", 6, 0, 0, 0.0),
            ("Attack Upgrade (+5%)", 7, 0, 0, 0.0),
        ] {
            let item = self.new_item(
                name,
                kind,
                if floor >= 5 { boss } else { initial },
                interval,
                if kind == 5 { self.max_health } else { 0 },
            );
            self.chest.push(item);
        }
    }

    /// Takes or atomically swaps one chest entry into a backpack position.
    pub fn take(&mut self, chest_index: i32, backpack_index: i32) -> bool {
        if self.health <= 0
            || !(0..3).contains(&backpack_index)
            || chest_index < 0
            || chest_index as usize >= self.chest.len()
            || self.chest[chest_index as usize].id == 0
        {
            return false;
        }
        let bag = backpack_index as usize;
        let chest = chest_index as usize;
        if self.backpack[bag].id == 0 {
            self.backpack[bag] = self.chest.remove(chest);
        } else {
            std::mem::swap(&mut self.backpack[bag], &mut self.chest[chest]);
        }
        true
    }

    /// Atomically exchanges compatible backpack/equipment items, even with a full backpack.
    pub fn equip(&mut self, backpack_index: i32, slot: i32) -> bool {
        if self.health <= 0 || !(0..3).contains(&backpack_index) || !(0..4).contains(&slot) {
            return false;
        }
        let item = &self.backpack[backpack_index as usize];
        if item.id == 0 || item.is_upgrade() || item.is_weapon() != (slot < 2) {
            return false;
        }
        std::mem::swap(
            &mut self.backpack[backpack_index as usize],
            &mut self.equipment[slot as usize],
        );
        true
    }

    /// Moves equipment into the first free backpack slot; failure preserves all items.
    pub fn unequip(&mut self, slot: i32) -> bool {
        if self.health <= 0 || !(0..4).contains(&slot) || self.equipment[slot as usize].id == 0 {
            return false;
        }
        let Some(empty) = self.backpack.iter().position(|item| item.id == 0) else {
            return false;
        };
        self.backpack[empty] = std::mem::take(&mut self.equipment[slot as usize]);
        true
    }

    /// Discards exactly one occupied backpack slot.
    pub fn discard(&mut self, index: i32) -> bool {
        if self.health <= 0 || !(0..3).contains(&index) || self.backpack[index as usize].id == 0 {
            return false;
        }
        self.backpack[index as usize] = Item::default();
        true
    }

    /// Consumes an upgrade once without refilling existing shields or fractions.
    pub fn use_upgrade(&mut self, index: i32) -> bool {
        if self.health <= 0
            || !(0..3).contains(&index)
            || !self.backpack[index as usize].is_upgrade()
        {
            return false;
        }
        if self.backpack[index as usize].kind == 6 {
            self.max_health += 10;
            self.health = self.max_health.min(self.health + 10);
        } else {
            self.attack_upgrades += 1;
            self.attack_multiplier = 1.0 + self.attack_upgrades as f32 * 0.05;
        }
        self.backpack[index as usize] = Item::default();
        true
    }

    /// Consumes an eligible item; the caller must validate grenade targeting first.
    pub fn use_item(&mut self, slot: i32) -> Option<Item> {
        if self.health <= 0 || !(2..=3).contains(&slot) {
            return None;
        }
        let item = &self.equipment[slot as usize];
        if item.id == 0 {
            return None;
        }
        match item.kind {
            3 if self.health < self.max_health => self.health = self.max_health,
            4 => {}
            _ => return None,
        }
        Some(std::mem::take(&mut self.equipment[slot as usize]))
    }

    /// Absorbs damage in item A/B order, then HP; even fully shielded damage resets the delay.
    pub fn apply_damage(&mut self, mut damage: i32) {
        if damage <= 0 || self.health <= 0 {
            return;
        }
        self.shield_delay_remaining = 3.0;
        self.damaged_this_update = true;
        for item in &mut self.equipment[2..4] {
            if item.id == 0 || item.kind != 5 {
                continue;
            }
            item.shield = self.max_health.min(item.shield);
            let absorbed = item.shield.min(damage);
            item.shield -= absorbed;
            damage -= absorbed;
        }
        self.health = 0.max(self.health - damage);
    }

    /// Refills living HP without changing shields or their shared delay.
    pub fn heal_fully(&mut self) {
        if self.health > 0 {
            self.health = self.max_health;
        }
    }

    /// Advances recovery once after all damage in a gameplay update; never while paused.
    pub fn advance_time(&mut self, dt: f32) {
        if self.damaged_this_update {
            self.damaged_this_update = false;
            return;
        }
        if dt <= 0.0 || !dt.is_finite() || self.health <= 0 {
            return;
        }
        let recovery_time = 0.0_f32.max(dt - self.shield_delay_remaining);
        self.shield_delay_remaining = 0.0_f32.max(self.shield_delay_remaining - dt);
        if recovery_time <= 0.0 {
            return;
        }
        for item in &mut self.equipment[2..4] {
            if item.id == 0 || item.kind != 5 {
                continue;
            }
            let recovered = item.shield_fraction + recovery_time * self.max_health as f32 * 0.1;
            let whole_points = (recovered + 0.00001).floor() as i32;
            item.shield = self.max_health.min(item.shield + whole_points);
            item.shield_fraction = if item.shield >= self.max_health {
                0.0
            } else {
                0.0_f32.max(recovered - whole_points as f32)
            };
        }
    }
}
