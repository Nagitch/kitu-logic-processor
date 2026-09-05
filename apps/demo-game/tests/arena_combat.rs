mod support;

#[test]
fn command_phase_events_preserve_lifecycle_order_without_duplicate_notifications() {
    use kitu_osc_ir::OscArg;
    let mut runtime = kitu_demo_game::build_arena_runtime().unwrap();
    for (id, address) in [
        (1, "start"),
        (1, "start"),
        (2, "start"),
        (3, "menu"),
        (4, "menu"),
        (5, "start"),
    ] {
        support::send(
            &mut runtime,
            "commands",
            id,
            &format!("/input/arena/{address}"),
            vec![],
        );
    }
    runtime.tick_once().unwrap();
    let messages: Vec<_> = runtime
        .drain_output_buffer()
        .into_iter()
        .flat_map(|b| b.messages)
        .collect();
    let mut transitions = Vec::new();
    for (order, message) in messages.iter().enumerate() {
        if message.address != "/game/arena/phase" {
            continue;
        }
        let OscArg::Str(json) = &message.args[0] else {
            panic!("phase JSON")
        };
        let event: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(event["tick"], 0);
        assert_eq!(event["order"], order);
        assert_eq!(messages[order + 1].address, "/ui/arena/command");
        transitions.push((
            event["previous"].as_i64().unwrap(),
            event["phase"].as_i64().unwrap(),
        ));
    }
    assert_eq!(transitions, [(0, 1), (1, 0), (0, 1)]);
}

#[test]
fn first_floor_combat_matches_every_frozen_csharp_state_field() {
    let (checkpoints, outcomes) = support::replay_reference("stock-eleven-death-retry", 470);
    assert!(checkpoints > 40);
    assert_eq!(outcomes, 16);
}

#[test]
fn queued_consumables_coalesce_clear_on_pause_and_never_replay_duplicates() {
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
            OscArg::Bool(true),
            OscArg::Float(0.0),
            OscArg::Float(5.0),
            OscArg::Bool(false),
            OscArg::Bool(false),
        ],
    );
    for _ in 0..75 {
        runtime.tick_once().unwrap();
        runtime.drain_output_buffer();
    }
    for (id, address, args) in [
        (2, "chest", vec![]),
        (3, "take", vec![OscArg::Int(9), OscArg::Int(0)]),
        (
            4,
            "equip",
            vec![OscArg::Int(9), OscArg::Int(0), OscArg::Int(2)],
        ),
        (5, "close", vec![]),
    ] {
        support::send(
            &mut runtime,
            "commands",
            id,
            &format!("/input/arena/{address}"),
            args,
        );
    }
    runtime.tick_once().unwrap();
    runtime.drain_output_buffer();
    assert_eq!(
        support::projection(&runtime)["inventory"]["equipment"][2]["id"],
        9
    );
    support::send(
        &mut runtime,
        "commands",
        6,
        "/input/arena/use",
        vec![OscArg::Int(2)],
    );
    support::send(&mut runtime, "commands", 7, "/input/arena/pause", vec![]);
    runtime.tick_once().unwrap();
    runtime.drain_output_buffer();
    support::send(&mut runtime, "commands", 8, "/input/arena/resume", vec![]);
    runtime.tick_once().unwrap();
    runtime.drain_output_buffer();
    assert_eq!(
        support::projection(&runtime)["inventory"]["equipment"][2]["id"],
        9
    );
    support::send(
        &mut runtime,
        "frames",
        2,
        "/input/arena/frame",
        vec![
            OscArg::Float(0.0),
            OscArg::Float(0.0),
            OscArg::Bool(true),
            OscArg::Float(0.0),
            OscArg::Float(5.0),
            OscArg::Bool(false),
            OscArg::Bool(false),
        ],
    );
    for id in [9, 9, 10] {
        support::send(
            &mut runtime,
            "commands",
            id,
            "/input/arena/use",
            vec![OscArg::Int(2)],
        );
    }
    runtime.tick_once().unwrap();
    let messages: Vec<_> = runtime
        .drain_output_buffer()
        .into_iter()
        .flat_map(|b| b.messages)
        .collect();
    assert_eq!(
        messages
            .iter()
            .filter(|m| m.address == "/ui/arena/use")
            .count(),
        1
    );
    assert_eq!(
        messages
            .iter()
            .filter(|m| m.address == "/game/arena/inventory")
            .count(),
        1
    );
    assert_eq!(
        support::projection(&runtime)["grenades"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        support::projection(&runtime)["inventory"]["equipment"][2]["id"],
        0
    );
    support::send(
        &mut runtime,
        "commands",
        10,
        "/input/arena/use",
        vec![OscArg::Int(2)],
    );
    runtime.tick_once().unwrap();
    assert!(!runtime
        .drain_output_buffer()
        .iter()
        .flat_map(|b| &b.messages)
        .any(|m| m.address == "/ui/arena/use"));
}
