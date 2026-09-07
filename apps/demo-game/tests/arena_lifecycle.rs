use kitu_demo_game::{arena::ArenaState, build_arena_runtime, DemoRuntime};
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_runtime::InputMetadata;

fn input(runtime: &mut DemoRuntime, id: u64, suffix: &str, args: Vec<OscArg>) {
    let mut bundle = OscBundle::new();
    let mut message = OscMessage::new(format!("/input/arena/{suffix}"));
    message.args = args;
    bundle.push(message);
    runtime.enqueue_tagged_input(
        bundle,
        InputMetadata {
            source: "test".into(),
            message_id: id,
            schema_version: 1,
        },
    );
}

fn state(runtime: &DemoRuntime) -> ArenaState {
    let projection = runtime.inspect_application();
    let OscArg::Str(json) = &projection[0].messages[0].args[0] else {
        panic!("JSON projection");
    };
    serde_json::from_str(json).unwrap()
}

fn frame(x: f32, y: f32) -> Vec<OscArg> {
    vec![
        OscArg::Float(x),
        OscArg::Float(y),
        OscArg::Bool(true),
        OscArg::Float(0.0),
        OscArg::Float(0.0),
        OscArg::Bool(false),
        OscArg::Bool(false),
    ]
}

#[test]
fn fixed_steps_match_reference_movement_clamp_and_aim_and_pause_clears_controls() {
    let mut runtime = build_arena_runtime().unwrap();
    input(&mut runtime, 1, "start", vec![]);
    input(&mut runtime, 2, "frame", frame(1.0, 1.0));
    let dt = 1.0_f32 / 60.0;
    let mut expected_x = 0.0_f32;
    let mut expected_y = -7.0_f32;
    let mut elapsed = 0.0_f32;
    for _ in 0..400 {
        expected_x = (expected_x + (1.0 / 2.0_f32.sqrt()) * (5.0 * dt)).clamp(-9.5, 9.5);
        expected_y = (expected_y + (1.0 / 2.0_f32.sqrt()) * (5.0 * dt)).clamp(-9.5, 9.5);
        elapsed += dt;
        runtime.tick_once().unwrap();
        runtime.drain_output_buffer();
    }
    let moving = state(&runtime);
    assert_eq!(moving.simulation_steps, 400);
    assert_eq!(moving.elapsed, elapsed);
    assert_eq!(moving.player_position.x, expected_x);
    assert_eq!(moving.player_position.y, expected_y);
    assert!((moving.aim_direction.x + 1.0 / 2.0_f32.sqrt()).abs() < 1e-4);
    input(&mut runtime, 3, "pause", vec![]);
    for _ in 0..120 {
        runtime.tick_once().unwrap();
        runtime.drain_output_buffer();
    }
    assert_eq!(state(&runtime).elapsed, elapsed);
    assert_eq!(state(&runtime).tick, 519);
    input(&mut runtime, 4, "resume", vec![]);
    runtime.tick_once().unwrap();
    assert_eq!(state(&runtime).player_position, moving.player_position);
    assert_eq!(state(&runtime).simulation_steps, 401);
    // A fresh frame moves; out-of-order delivery cannot reintroduce stale movement.
    input(&mut runtime, 6, "frame", frame(-1.0, 0.0));
    input(&mut runtime, 5, "frame", frame(1.0, 0.0));
    runtime.tick_once().unwrap();
    assert!(state(&runtime).player_position.x < moving.player_position.x);
}

#[test]
fn invalid_batch_is_atomic_and_command_retries_retain_their_original_result() {
    let mut runtime = build_arena_runtime().unwrap();
    input(&mut runtime, 1, "start", vec![]);
    input(&mut runtime, 2, "frame", frame(f32::NAN, 0.0));
    assert!(runtime.tick_once().is_err());
    assert_eq!(state(&runtime).phase, 0);
    input(&mut runtime, 1, "start", vec![]);
    runtime.tick_once().unwrap();
    runtime.drain_output_buffer();
    input(&mut runtime, 1, "start", vec![]);
    input(&mut runtime, 1, "menu", vec![]);
    runtime.tick_once().unwrap();
    let outcomes: Vec<serde_json::Value> = runtime
        .drain_output_buffer()
        .into_iter()
        .flat_map(|bundle| bundle.messages)
        .filter(|message| message.address == "/ui/arena/command")
        .map(|message| {
            let OscArg::Str(json) = &message.args[0] else {
                panic!("JSON outcome")
            };
            serde_json::from_str(json).unwrap()
        })
        .collect();
    assert_eq!(outcomes[0]["appliedTick"], 0);
    assert_eq!(outcomes[0]["duplicate"], true);
    assert_eq!(outcomes[1]["code"], "id_conflict");
    assert_eq!(outcomes[1]["appliedTick"], -1);
    assert_eq!(state(&runtime).phase, 1);
    assert_eq!(state(&runtime).simulation_steps, 2);
}

#[test]
fn resetting_legacy_world_objects_preserves_application_state_and_control_queue() {
    let mut runtime = build_arena_runtime().unwrap();
    input(&mut runtime, 1, "start", vec![]);
    runtime.tick_once().unwrap();
    input(&mut runtime, 2, "disconnect", vec![]);
    runtime.reset_world_objects();
    runtime.tick_once().unwrap();
    assert_eq!(state(&runtime).phase, 1);
    assert_eq!(state(&runtime).overlay, "pause");
    assert_eq!(state(&runtime).simulation_steps, 1);
}

#[test]
fn stock_reference_approach_matches_frozen_csharp_checkpoints() {
    let reference = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../kitu-integration-runner/scenarios/arena/reference/stock-eleven-death-retry");
    let scenario: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(reference.join("scenario.json")).unwrap())
            .unwrap();
    let checkpoints: Vec<serde_json::Value> =
        std::fs::read_to_string(reference.join("expected.ndjson"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
    let mut runtime = build_arena_runtime().unwrap();
    let mut compared = 0;
    // This migration slice ends before the first chest interaction (tick 75).
    // Inputs and expected coordinates come from the unchanged C# capture, not a new bot.
    for step in scenario["steps"].as_array().unwrap().iter().take(75) {
        let tick = step["tick"].as_u64().unwrap();
        for command in step["commands"].as_array().unwrap() {
            input(
                &mut runtime,
                command["id"].as_u64().unwrap(),
                "start",
                vec![],
            );
            assert_eq!(command["address"], "/input/arena/start");
        }
        let f = &step["frame"];
        input(
            &mut runtime,
            100_000 + tick,
            "frame",
            vec![
                OscArg::Float(f["Move"]["x"].as_f64().unwrap() as f32),
                OscArg::Float(f["Move"]["y"].as_f64().unwrap() as f32),
                OscArg::Bool(f["HasAim"].as_bool().unwrap()),
                OscArg::Float(f["AimPoint"]["x"].as_f64().unwrap() as f32),
                OscArg::Float(f["AimPoint"]["y"].as_f64().unwrap() as f32),
                OscArg::Bool(false),
                OscArg::Bool(false),
            ],
        );
        runtime.tick_once().unwrap();
        runtime.drain_output_buffer();
        if let Some(expected) = checkpoints.iter().find(|value| value["tick"] == tick) {
            let actual = state(&runtime);
            for (name, value) in [
                ("tick", actual.tick),
                ("simulationSteps", actual.simulation_steps as i64),
                ("phase", actual.phase as i64),
                ("floor", actual.floor as i64),
            ] {
                assert_eq!(expected[name], value, "tick {tick}: {name}");
            }
            assert_eq!(expected["overlay"], actual.overlay);
            for (path, value) in [
                ("/elapsed", actual.elapsed),
                ("/playerPosition/x", actual.player_position.x),
                ("/playerPosition/y", actual.player_position.y),
                ("/aimDirection/x", actual.aim_direction.x),
                ("/aimDirection/y", actual.aim_direction.y),
            ] {
                assert!(
                    (expected.pointer(path).unwrap().as_f64().unwrap() - value as f64).abs()
                        <= 1e-4,
                    "tick {tick}: {path}"
                );
            }
            compared += 1;
        }
    }
    assert_eq!(compared, 2);
}

#[test]
fn message_identity_is_shared_across_frames_and_discrete_commands() {
    let mut runtime = build_arena_runtime().unwrap();
    input(&mut runtime, 1, "start", vec![]);
    input(&mut runtime, 2, "frame", frame(1.0, 0.0));
    runtime.tick_once().unwrap();
    let before = state(&runtime).player_position.x;
    runtime.drain_output_buffer();
    input(&mut runtime, 2, "menu", vec![]); // Frame ID cannot become an operation.
    input(&mut runtime, 1, "frame", frame(-1.0, 0.0)); // Operation ID cannot replace controls.
    runtime.tick_once().unwrap();
    assert_eq!(state(&runtime).phase, 1);
    assert!(state(&runtime).player_position.x > before);
    let conflicts = runtime
        .drain_output_buffer()
        .into_iter()
        .flat_map(|b| b.messages)
        .filter(|m| m.address == "/ui/arena/command")
        .count();
    assert_eq!(conflicts, 2);
    input(&mut runtime, 1, "start", vec![]); // A real command retry is still recognized.
    runtime.tick_once().unwrap();
    assert_eq!(state(&runtime).simulation_steps, 3);
}

#[test]
fn deferred_commands_validate_the_complete_declared_payload_before_admission() {
    let mut runtime = build_arena_runtime().unwrap();
    let meta = InputMetadata {
        source: "test".into(),
        message_id: 1,
        schema_version: 1,
    };
    for (suffix, count) in [
        ("inventory", 0),
        ("chest", 0),
        ("close", 0),
        ("use", 1),
        ("take", 2),
        ("discard", 2),
        ("upgrade", 2),
        ("unequip", 2),
        ("equip", 3),
    ] {
        let mut message = OscMessage::new(format!("/input/arena/{suffix}"));
        message.args = vec![OscArg::Int(1); count];
        kitu_demo_game::arena::validate_input(&message, &meta).unwrap();
        message.args.push(OscArg::Int(1));
        assert!(
            kitu_demo_game::arena::validate_input(&message, &meta).is_err(),
            "{suffix} extra argument"
        );
        message.args = vec![OscArg::Float(1.0); count.max(1)];
        let mut bundle = OscBundle::new();
        bundle.push(message);
        assert!(
            runtime
                .try_enqueue_input(bundle, Some(meta.clone()))
                .is_err(),
            "{suffix} wrong types"
        );
    }
    assert!(
        kitu_demo_game::arena::validate_input(&OscMessage::new("/input/arena/unknown"), &meta)
            .is_err()
    );
    input(&mut runtime, 1, "start", vec![]);
    runtime.tick_once().unwrap();
    assert_eq!(state(&runtime).phase, 1);
}
