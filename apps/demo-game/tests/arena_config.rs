use kitu_data_tmd::tables::{DataSourceDefinition, FormulaTableCellContent, TanuDocument};
use kitu_demo_game::{
    arena::{
        self,
        config::{ArenaConfig, ContentVersion},
    },
    build_arena_runtime, DemoRuntime,
};
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_runtime::InputMetadata;

fn command(runtime: &mut DemoRuntime, id: u64, suffix: &str, args: Vec<OscArg>) {
    let mut bundle = OscBundle::new();
    let mut message = OscMessage::new(format!("/input/arena/{suffix}"));
    message.args = args;
    bundle.push(message);
    runtime
        .try_enqueue_input(
            bundle,
            Some(InputMetadata {
                source: "config-test".into(),
                message_id: id,
                schema_version: 1,
            }),
        )
        .unwrap();
}

fn state(runtime: &DemoRuntime) -> arena::ArenaState {
    let projection = runtime.inspect_application();
    let OscArg::Str(json) = &projection[0].messages[0].args[0] else {
        panic!()
    };
    serde_json::from_str(json).unwrap()
}

fn tick(runtime: &mut DemoRuntime) -> Vec<OscMessage> {
    runtime.tick_once().unwrap();
    runtime
        .drain_output_buffer()
        .into_iter()
        .flat_map(|bundle| bundle.messages)
        .collect()
}

fn movement(x: f32, y: f32) -> Vec<OscArg> {
    vec![
        OscArg::Float(x),
        OscArg::Float(y),
        OscArg::Bool(false),
        OscArg::Float(0.0),
        OscArg::Float(0.0),
        OscArg::Bool(false),
        OscArg::Bool(false),
    ]
}

#[test]
fn staging_preserves_active_controls_and_freezes_values_only_when_next_run_starts() {
    let mut runtime = build_arena_runtime().unwrap();
    let original = arena::inspect_content(&runtime).unwrap().pending;
    command(&mut runtime, 1, "start", vec![]);
    command(&mut runtime, 2, "frame", movement(1.0, 0.0));
    let output = tick(&mut runtime);
    let saved_event = output
        .iter()
        .find(|message| message.address == "/game/arena/run")
        .unwrap()
        .clone();
    let mut edited = ArenaConfig::default();
    edited.items[0].damage = 37;
    edited.enemies[0].health = 71;
    edited.enemies[0].speed = 0.0;
    edited.difficulty.health_growth = 0.5;
    edited.chests.retain(|entry| entry.item_id == "quick-blade");
    let candidate = ContentVersion::from_tmd(&edited.to_tmd().unwrap()).unwrap();
    let before = state(&runtime);
    arena::stage_content(&mut runtime, candidate.clone(), 1).unwrap();
    assert_eq!(arena::inspect_content(&runtime).unwrap().pending, original);
    tick(&mut runtime);
    assert!(state(&runtime).player_position.x > before.player_position.x);
    assert_eq!(state(&runtime).inventory.equipment[0].damage, 20);
    assert_eq!(
        arena::inspect_content(&runtime).unwrap().active,
        Some(original.clone())
    );
    assert_eq!(arena::inspect_content(&runtime).unwrap().pending, candidate);

    command(&mut runtime, 3, "pause", vec![]);
    tick(&mut runtime);
    let paused = state(&runtime);
    arena::stage_content(&mut runtime, candidate.clone(), 1).unwrap();
    let repeated = tick(&mut runtime);
    assert_eq!(state(&runtime).elapsed, paused.elapsed);
    assert!(!repeated
        .iter()
        .any(|message| message.address == "/ui/arena/content"));
    let mut corrupt = candidate.clone();
    corrupt.values.items[0].damage = 100;
    assert!(arena::stage_content(&mut runtime, corrupt, 2).is_err());
    assert_eq!(arena::inspect_content(&runtime).unwrap().pending, candidate);

    command(&mut runtime, 4, "menu", vec![]);
    command(&mut runtime, 5, "start", vec![]);
    let next = tick(&mut runtime);
    assert_eq!(arena::inspect_content(&runtime).unwrap().run, 2);
    assert_eq!(
        arena::inspect_content(&runtime).unwrap().active,
        Some(candidate.clone())
    );
    assert_eq!(state(&runtime).inventory.equipment[0].damage, 37);
    assert_eq!(state(&runtime).inventory.chest.len(), 1);
    assert_eq!(
        next.iter()
            .filter(|message| message.address == "/game/arena/run")
            .count(),
        1
    );
    let OscArg::Str(json) = &saved_event.args[0] else {
        panic!()
    };
    let saved: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(saved["content"]["hash"], original.hash);
    assert_eq!(saved["content"]["values"]["items"][0]["damage"], 20);

    command(&mut runtime, 6, "frame", movement(0.0, 1.0));
    for _ in 0..230 {
        tick(&mut runtime);
        if state(&runtime).phase == 3 {
            break;
        }
    }
    let combat = state(&runtime);
    assert_eq!(combat.phase, 3);
    assert!(combat
        .enemies
        .iter()
        .all(|enemy| enemy.max_health == 71 && enemy.speed == 0.0));
}

#[test]
fn content_contract_rejects_untrusted_producers_versions_and_forged_hashes() {
    let runtime = build_arena_runtime().unwrap();
    let candidate = arena::inspect_content(&runtime).unwrap().pending;
    let mut message = OscMessage::new("/input/arena/config");
    message
        .args
        .push(OscArg::Str(serde_json::to_string(&candidate).unwrap()));
    assert!(arena::validate_input(
        &message,
        &InputMetadata {
            source: "unity".into(),
            message_id: 1,
            schema_version: 1
        }
    )
    .is_err());
    let mut forged = candidate.clone();
    forged.tanu_revision = "unknown".into();
    assert!(forged.validate().is_err());
    let mut forged = candidate;
    forged.hash = "0".repeat(64);
    assert!(forged.validate().is_err());
}

#[test]
fn real_tmd_evaluation_preserves_defaults_and_changes_formula_values() {
    let config = ArenaConfig::default();
    let first = config.to_tmd().unwrap();
    let second = config.to_tmd().unwrap();
    assert_eq!(ArenaConfig::from_tmd(&first).unwrap(), config);
    assert_eq!(
        ArenaConfig::from_tmd(&second).unwrap().hash().unwrap(),
        config.hash().unwrap()
    );
    let mut doc = TanuDocument::read(&first).unwrap();
    let mut sources = doc.sources().unwrap();
    let DataSourceDefinition::FormulaTable { rows, columns, .. } =
        sources.sources.get_mut("items").unwrap()
    else {
        panic!("managed table")
    };
    let damage = columns.iter().position(|c| c.name == "damage").unwrap();
    rows[1].cells[damage].content = FormulaTableCellContent::Formula {
        expression: "10 + 15".into(),
    };
    doc.set_sources(sources).unwrap();
    let changed = ArenaConfig::from_tmd(&doc.bytes().unwrap()).unwrap();
    assert_eq!(changed.items[1].damage, 25);
    assert_ne!(changed.hash().unwrap(), config.hash().unwrap());
    assert_eq!(ArenaConfig::from_tmd(&first).unwrap().items[1].damage, 20);
}

#[test]
fn invalid_values_identities_and_formula_errors_are_rejected_as_whole_candidates() {
    let original = ArenaConfig::default();
    for change in [0, 1, 2, 3, 4, 5] {
        let mut config = original.clone();
        match change {
            0 => config.items[1].interval = 0.0,
            1 => config.items[1].name = config.items[0].name.clone(),
            2 => config.enemies[0].health = 0,
            3 => config.difficulty.health_growth = f32::NAN,
            4 => config.chests[0].item_id = "missing".into(),
            _ => config.enemies[1].kind = 0,
        }
        assert!(config.validate().is_err());
        assert!(config.hash().is_err());
    }
    let mut doc = TanuDocument::read(&original.to_tmd().unwrap()).unwrap();
    let mut sources = doc.sources().unwrap();
    let DataSourceDefinition::FormulaTable { rows, columns, .. } =
        sources.sources.get_mut("items").unwrap()
    else {
        panic!("managed table")
    };
    let damage = columns.iter().position(|c| c.name == "damage").unwrap();
    rows[1].cells[damage].content = FormulaTableCellContent::Formula {
        expression: "1 / 0".into(),
    };
    doc.set_sources(sources).unwrap();
    assert!(ArenaConfig::from_tmd(&doc.bytes().unwrap()).is_err());
    assert!(ArenaConfig::from_tmd(b"damage: 100").is_err());
    let mut empty = original;
    empty.chests.clear();
    assert_eq!(
        ArenaConfig::from_tmd(&empty.to_tmd().unwrap()).unwrap(),
        empty
    );
}
