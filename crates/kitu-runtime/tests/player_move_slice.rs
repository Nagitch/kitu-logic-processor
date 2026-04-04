use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_runtime::build_runtime;
use kitu_transport::LocalChannel;

#[test]
fn player_move_slice_runs_end_to_end() {
    let mut runtime = build_runtime(LocalChannel::default());

    let mut move_message = OscMessage::new("/input/move");
    move_message.push_arg(OscArg::Str("player:local".to_string()));
    move_message.push_arg(OscArg::Float(1.5));
    move_message.push_arg(OscArg::Float(2.0));

    let mut input = OscBundle::new();
    input.push(move_message);
    runtime.enqueue_input(input);

    runtime.tick_once().expect("tick should succeed");

    let outputs = runtime.drain_output_buffer();
    assert_eq!(outputs.len(), 1);

    let render = &outputs[0].messages[0];
    assert_eq!(render.address, "/render/player/transform");
    assert_eq!(
        render.args,
        vec![
            OscArg::Str("player:local".to_string()),
            OscArg::Int64(0),
            OscArg::Float(1.5),
            OscArg::Float(2.0),
            OscArg::Float(0.0),
        ]
    );
}

#[test]
fn player_move_slice_accumulates_over_multiple_ticks() {
    let mut runtime = build_runtime(LocalChannel::default());

    let mut first = OscMessage::new("/input/move");
    first.push_arg(OscArg::Str("player:local".to_string()));
    first.push_arg(OscArg::Float(1.0));
    first.push_arg(OscArg::Float(0.0));

    let mut second = OscMessage::new("/input/move");
    second.push_arg(OscArg::Str("player:local".to_string()));
    second.push_arg(OscArg::Float(-0.5));
    second.push_arg(OscArg::Float(3.0));

    let mut first_bundle = OscBundle::new();
    first_bundle.push(first);
    runtime.enqueue_input(first_bundle);
    runtime.tick_once().expect("first tick should succeed");

    let mut second_bundle = OscBundle::new();
    second_bundle.push(second);
    runtime.enqueue_input(second_bundle);
    runtime.tick_once().expect("second tick should succeed");

    let outputs = runtime.drain_output_buffer();
    assert_eq!(outputs.len(), 2);

    let second_render = &outputs[1].messages[0];
    assert_eq!(
        second_render.args,
        vec![
            OscArg::Str("player:local".to_string()),
            OscArg::Int64(1),
            OscArg::Float(0.5),
            OscArg::Float(3.0),
            OscArg::Float(0.0),
        ]
    );
}
