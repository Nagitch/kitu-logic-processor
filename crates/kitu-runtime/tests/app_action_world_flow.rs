use std::collections::HashMap;

use kitu_app_actions::ActionValue;
use kitu_osc_ir::OscArg;
use kitu_runtime::{build_runtime, WorldTransform};
use kitu_transport::LocalChannel;

#[test]
fn app_action_world_flow_updates_runtime_state_and_outputs() {
    let mut runtime = build_runtime(LocalChannel::default());

    runtime
        .run_app_action(
            "spawn-object",
            &HashMap::from([
                ("kind".to_string(), ActionValue::String("enemy".to_string())),
                ("x".to_string(), ActionValue::Float(1.0)),
                ("y".to_string(), ActionValue::Float(2.0)),
                ("z".to_string(), ActionValue::Float(3.0)),
            ]),
        )
        .expect("spawn app action should succeed");

    let spawned = runtime.inspect_world_state().objects;
    assert_eq!(spawned.len(), 1);
    assert_eq!(spawned[0].id, "obj-1");
    assert_eq!(spawned[0].kind, "enemy");
    assert_eq!(spawned[0].transform, WorldTransform::new(1.0, 2.0, 3.0));

    runtime.tick_once().expect("spawn output tick should succeed");
    let spawn_outputs = runtime.drain_output_buffer();
    assert_eq!(spawn_outputs.len(), 1);
    assert_eq!(spawn_outputs[0].messages[0].address, "/render/player/transform");
    assert_eq!(
        spawn_outputs[0].messages[0].args,
        vec![
            OscArg::Str("obj-1".to_string()),
            OscArg::Int64(0),
            OscArg::Float(1.0),
            OscArg::Float(3.0),
            OscArg::Float(0.0),
        ]
    );

    runtime
        .run_app_action(
            "move-object",
            &HashMap::from([
                ("id".to_string(), ActionValue::String("obj-1".to_string())),
                ("x".to_string(), ActionValue::Float(2.0)),
                ("y".to_string(), ActionValue::Float(5.0)),
                ("z".to_string(), ActionValue::Float(4.0)),
            ]),
        )
        .expect("move app action should succeed");

    let moved = runtime.inspect_world_state().objects;
    assert_eq!(moved.len(), 1);
    assert_eq!(moved[0].transform, WorldTransform::new(2.0, 5.0, 4.0));

    runtime.tick_once().expect("move output tick should succeed");
    let move_outputs = runtime.drain_output_buffer();
    assert_eq!(move_outputs.len(), 1);
    assert_eq!(
        move_outputs[0].messages[0].args,
        vec![
            OscArg::Str("obj-1".to_string()),
            OscArg::Int64(1),
            OscArg::Float(2.0),
            OscArg::Float(4.0),
            OscArg::Float(0.0),
        ]
    );
}
