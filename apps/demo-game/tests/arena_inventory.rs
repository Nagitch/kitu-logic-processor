mod support;
use kitu_demo_game::arena::inventory::{Inventory, Item};

fn ready() -> Inventory {
    let mut inv = Inventory::default();
    inv.create_chest(0);
    inv
}
fn take_kind(inv: &mut Inventory, kind: i32, bag: i32) {
    let index = inv.chest.iter().position(|item| item.kind == kind).unwrap();
    assert!(inv.take(index as i32, bag));
}
fn equip_kind(inv: &mut Inventory, kind: i32, slot: i32) -> i32 {
    take_kind(inv, kind, 0);
    assert!(inv.equip(0, slot));
    inv.equipment[slot as usize].id
}
fn items(inv: &Inventory) -> Vec<Item> {
    let mut items: Vec<_> = inv
        .backpack
        .iter()
        .chain(&inv.equipment)
        .chain(&inv.chest)
        .filter(|i| i.id != 0)
        .cloned()
        .collect();
    items.sort_by_key(|item| item.id);
    items
}

#[test]
fn preparation_and_stock_loadout_match_frozen_csharp_inventory_and_results() {
    assert_eq!(support::replay_reference("preparation", 28), (12, 12));
    let (checkpoints, outcomes) = support::replay_reference("stock-eleven-death-retry", 185);
    assert!(checkpoints > 10 && outcomes > 10);
}

#[test]
fn chest_tables_repeat_and_swaps_conserve_owned_instances_when_full() {
    let mut inv = ready();
    assert_eq!(inv.chest.len(), 11);
    assert_eq!(inv.chest.iter().filter(|i| i.is_weapon()).count(), 6);
    assert_eq!(inv.chest.iter().filter(|i| i.is_upgrade()).count(), 2);
    let original = items(&inv);
    for index in 0..3 {
        assert!(inv.take(0, index));
    }
    assert!(inv.take(0, 1));
    assert!(inv.equip(1, 0));
    assert_eq!(items(&inv), original);
    inv.create_chest(5);
    let damage: Vec<_> = inv.chest.iter().map(|i| i.damage).collect();
    inv.create_chest(100);
    assert_eq!(
        inv.chest.iter().map(|i| i.damage).collect::<Vec<_>>(),
        damage
    );
}

#[test]
fn invalid_transfers_are_atomic_and_unequip_uses_first_empty_slot() {
    let mut inv = ready();
    take_kind(&mut inv, 3, 0);
    take_kind(&mut inv, 6, 1);
    take_kind(&mut inv, 1, 2);
    let before = inv.clone();
    assert!(!inv.equip(0, 0));
    assert!(!inv.equip(1, 2));
    assert!(!inv.equip(2, 2));
    assert!(!inv.equip(-1, 2));
    assert!(!inv.equip(0, 99));
    assert!(!inv.unequip(0));
    assert!(!inv.take(-1, 0));
    assert!(!inv.take(0, 3));
    assert!(!inv.use_upgrade(2));
    assert!(!inv.discard(3));
    assert_eq!(inv, before);
    assert!(inv.discard(1));
    assert!(inv.unequip(0));
    assert_eq!(inv.backpack[1], before.equipment[0]);
    assert_eq!(inv.equipment[0].id, 0);
    assert!(inv.discard(1));
    assert!(!inv.discard(1));
}

#[test]
fn upgrades_consume_once_and_preserve_shield_charge_and_delay() {
    let mut inv = ready();
    equip_kind(&mut inv, 5, 2);
    inv.apply_damage(125);
    take_kind(&mut inv, 6, 0);
    assert!(inv.use_upgrade(0));
    assert!(!inv.use_upgrade(0));
    assert_eq!(
        (inv.max_health, inv.health, inv.equipment[2].shield),
        (110, 85, 0)
    );
    take_kind(&mut inv, 7, 0);
    assert!(inv.use_upgrade(0));
    assert!(!inv.use_upgrade(0));
    assert_eq!(inv.attack_multiplier, 1.05);
    assert_eq!(inv.shield_delay_remaining, 3.0);
    inv.create_chest(5);
    assert_eq!(inv.chest.iter().find(|i| i.kind == 5).unwrap().shield, 110);
}

#[test]
fn medkits_keep_full_hp_items_and_consumption_never_autofills_equipment() {
    let mut inv = ready();
    let medkit = equip_kind(&mut inv, 3, 2);
    equip_kind(&mut inv, 5, 3);
    assert!(inv.use_item(2).is_none());
    assert_eq!(inv.equipment[2].id, medkit);
    inv.apply_damage(130);
    assert_eq!(inv.use_item(2).unwrap().id, medkit);
    assert_eq!(inv.health, 100);
    assert_eq!(inv.equipment[3].shield, 0);
    assert_eq!(inv.shield_delay_remaining, 3.0);
    let grenade = equip_kind(&mut inv, 4, 2);
    inv.create_chest(5);
    take_kind(&mut inv, 4, 0);
    let spare = inv.backpack[0].id;
    assert_eq!(inv.use_item(2).unwrap().id, grenade);
    assert_eq!(inv.backpack[0].id, spare);
    assert_eq!(inv.equipment[2].id, 0);
    assert!(inv.use_item(2).is_none());
    equip_kind(&mut inv, 3, 2);
    inv.create_chest(5);
    equip_kind(&mut inv, 3, 3);
    inv.apply_damage(15);
    assert!(inv.use_item(2).is_some());
    assert!(inv.use_item(3).is_none());
}

#[test]
fn shields_absorb_in_slot_order_and_damage_blocks_the_whole_updates_recovery() {
    let mut inv = ready();
    equip_kind(&mut inv, 5, 2);
    inv.create_chest(5);
    equip_kind(&mut inv, 5, 3);
    assert!(inv.use_item(2).is_none());
    inv.apply_damage(130);
    assert_eq!(
        (inv.equipment[2].shield, inv.equipment[3].shield, inv.health),
        (0, 70, 100)
    );
    inv.advance_time(100.0);
    assert_eq!(inv.equipment[3].shield, 70);
    assert_eq!(inv.shield_delay_remaining, 3.0);
    inv.advance_time(2.5);
    inv.advance_time(0.5);
    assert_eq!(inv.equipment[3].shield, 70);
    inv.advance_time(1.0);
    assert_eq!(inv.equipment[2].shield, 10);
    assert_eq!(inv.equipment[3].shield, 80);
    inv.apply_damage(120);
    assert_eq!(inv.health, 70);
    inv.heal_fully();
    assert_eq!(inv.health, 100);
    assert_eq!((inv.equipment[2].shield, inv.equipment[3].shield), (0, 0));
}

#[test]
fn fractional_recovery_survives_transfers_and_only_equipped_items_recover() {
    let mut inv = ready();
    let id = equip_kind(&mut inv, 5, 2);
    inv.apply_damage(50);
    inv.advance_time(0.0);
    inv.advance_time(3.05);
    assert!((inv.equipment[2].shield_fraction - 0.5).abs() <= 1e-5);
    assert!(inv.unequip(2));
    inv.advance_time(5.0);
    assert_eq!(inv.backpack[0].shield, 50);
    assert!(inv.take(0, 0));
    inv.advance_time(5.0);
    assert_eq!(inv.chest[0].id, id);
    assert_eq!(inv.chest[0].shield, 50);
    assert!(inv.take(0, 0));
    assert!(inv.equip(0, 3));
    inv.advance_time(0.05);
    assert_eq!(inv.equipment[3].shield, 51);
    assert!(inv.equipment[3].shield_fraction <= 1e-5);
    inv.apply_damage(100);
    inv.advance_time(0.0);
    inv.advance_time(3.0);
    for _ in 0..100 {
        inv.advance_time(0.01);
    }
    assert_eq!(inv.equipment[3].shield, 10);
    take_kind(&mut inv, 6, 0);
    inv.use_upgrade(0);
    inv.advance_time(1.0);
    assert_eq!(inv.equipment[3].shield, 21);
    inv.advance_time(30.0);
    assert_eq!(inv.equipment[3].shield, 110);
}

#[test]
fn dead_inventory_cannot_revive_consume_transfer_or_regenerate() {
    let mut inv = ready();
    equip_kind(&mut inv, 3, 2);
    take_kind(&mut inv, 6, 0);
    inv.apply_damage(200);
    inv.advance_time(0.0);
    let before = inv.clone();
    inv.heal_fully();
    inv.advance_time(100.0);
    inv.apply_damage(10);
    assert!(inv.use_item(2).is_none());
    assert!(!inv.use_upgrade(0));
    assert!(!inv.take(0, 1));
    assert!(!inv.equip(0, 3));
    assert!(!inv.unequip(2));
    assert!(!inv.discard(0));
    assert_eq!(inv, before);
}

#[test]
fn runtime_upgrades_emit_once_and_rejected_destinations_preserve_ownership() {
    use kitu_osc_ir::OscArg;
    let mut runtime = kitu_demo_game::build_arena_runtime().unwrap();
    support::send(&mut runtime, "commands", 1, "/input/arena/start", vec![]);
    let length = 58.0_f32.sqrt();
    support::send(
        &mut runtime,
        "frames",
        1,
        "/input/arena/frame",
        vec![
            OscArg::Float(-3.0 / length),
            OscArg::Float(7.0 / length),
            OscArg::Bool(false),
            OscArg::Float(0.0),
            OscArg::Float(0.0),
            OscArg::Bool(false),
            OscArg::Bool(false),
        ],
    );
    for _ in 0..75 {
        runtime.tick_once().unwrap();
        runtime.drain_output_buffer();
    }
    support::send(&mut runtime, "commands", 2, "/input/arena/chest", vec![]);
    runtime.tick_once().unwrap();
    runtime.drain_output_buffer();
    assert_eq!(support::projection(&runtime)["overlay"], "chest");
    support::send(
        &mut runtime,
        "commands",
        3,
        "/input/arena/take",
        vec![OscArg::Int(11), OscArg::Int(0)],
    );
    for _ in 0..2 {
        support::send(
            &mut runtime,
            "commands",
            4,
            "/input/arena/upgrade",
            vec![OscArg::Int(11), OscArg::Int(0)],
        );
    }
    support::send(
        &mut runtime,
        "commands",
        5,
        "/input/arena/take",
        vec![OscArg::Int(2), OscArg::Int(3)],
    );
    runtime.tick_once().unwrap();
    let state = support::projection(&runtime);
    assert_eq!(state["inventory"]["maxHealth"], 110);
    assert_eq!(state["inventory"]["backpack"][0]["id"], 0);
    assert_eq!(state["inventory"]["chest"][0]["id"], 2);
    let mut events = Vec::new();
    let mut outcomes = Vec::new();
    for message in runtime
        .drain_output_buffer()
        .into_iter()
        .flat_map(|b| b.messages)
    {
        let OscArg::Str(json) = &message.args[0] else {
            continue;
        };
        let body: serde_json::Value = serde_json::from_str(json).unwrap();
        if message.address == "/game/arena/inventory" {
            events.push(body);
        } else if message.address == "/ui/arena/command" {
            outcomes.push(body);
        }
    }
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["messageId"], 3);
    assert_eq!(events[1]["messageId"], 4);
    assert_eq!(events[1]["tick"], 76);
    assert_eq!(events[1]["itemId"], 11);
    assert_eq!(events[1]["inventory"]["maxHealth"], 110);
    assert_eq!(outcomes[2]["duplicate"], true);
    assert_eq!(outcomes[2]["accepted"], true);
    assert_eq!(outcomes[3]["code"], "invalid_target");
}
