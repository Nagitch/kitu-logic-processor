use kitu_core::{KituError, Result};
use kitu_ecs::EcsWorld;
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_runtime::{
    ApplicationTick, InputMetadata, Runtime, RuntimeApplication, RuntimeConfig, RuntimeInput,
};
use kitu_transport::LocalChannel;

struct Counter;

impl RuntimeApplication for Counter {
    fn validate_inputs(&self, inputs: &[RuntimeInput]) -> Result<()> {
        if inputs
            .iter()
            .flat_map(|i| &i.bundle.messages)
            .any(|m| m.address == "/bad")
        {
            return Err(KituError::InvalidInput("bad application command"));
        }
        Ok(())
    }

    fn tick(&mut self, world: &mut EcsWorld, context: ApplicationTick<'_>) -> Vec<OscBundle> {
        *world.resource_mut::<u64>().unwrap() += 1;
        let mut outputs = self.snapshot(world);
        for input in context.inputs {
            if let Some(meta) = &input.metadata {
                let mut message = OscMessage::new("/receipt");
                message.push_arg(OscArg::Str(meta.source.clone()));
                message.push_arg(OscArg::Int64(meta.message_id as i64));
                message.push_arg(OscArg::Int64(context.tick.get() as i64));
                message.push_arg(OscArg::Float(context.dt));
                message.push_arg(OscArg::Int64(input.sequence as i64));
                outputs[0].push(message);
            }
        }
        outputs
    }

    fn snapshot(&self, world: &EcsWorld) -> Vec<OscBundle> {
        let mut output = OscBundle::new();
        let mut state = OscMessage::new("/counter");
        state.push_arg(OscArg::Int64(*world.resource::<u64>().unwrap() as i64));
        output.push(state);
        vec![output]
    }
}

fn runtime() -> Runtime<LocalChannel> {
    let mut runtime = Runtime::new(RuntimeConfig::default_60hz(), LocalChannel::connected());
    runtime.world_mut().insert_resource(10_u64);
    runtime.install_application(Counter).unwrap();
    runtime
}

#[test]
fn application_persists_without_inputs_and_preserves_tagged_input_timing() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.inspect_application()[0].messages[0].args,
        vec![OscArg::Int64(10)]
    );
    runtime.enqueue_tagged_input(
        OscBundle::new(),
        InputMetadata {
            source: "client-a".into(),
            message_id: 7,
            schema_version: 1,
        },
    );
    assert!(runtime.drain_output_buffer().is_empty());
    runtime.tick_once().unwrap();
    let output = runtime.drain_output_buffer();
    assert_eq!(output[0].messages[0].args, vec![OscArg::Int64(11)]);
    assert_eq!(
        output[0].messages[1].args,
        vec![
            OscArg::Str("client-a".into()),
            OscArg::Int64(7),
            OscArg::Int64(0),
            OscArg::Float(1.0 / 60.0),
            OscArg::Int64(0)
        ]
    );
    runtime.tick_once().unwrap();
    assert_eq!(
        runtime.drain_output_buffer()[0].messages[0].args,
        vec![OscArg::Int64(12)]
    );
    assert_eq!(runtime.current_tick().get(), 2);
}

#[test]
fn malformed_application_batch_cannot_partially_advance_or_poison_following_ticks() {
    let mut runtime = runtime();
    let mut input = OscBundle::new();
    input.push(OscMessage::new("/bad"));
    runtime.enqueue_input(input);
    assert!(runtime.tick_once().is_err());
    assert_eq!(runtime.world_mut().resource::<u64>(), Some(&10));
    assert_eq!(runtime.current_tick().get(), 0);
    assert!(runtime.drain_output_buffer().is_empty());
    runtime.tick_once().unwrap();
    assert_eq!(runtime.world_mut().resource::<u64>(), Some(&11));
    assert!(runtime.install_application(Counter).is_err());
}

#[test]
fn resources_are_type_and_runtime_local_and_application_installation_is_guarded() {
    let mut first = runtime();
    let mut second = runtime();
    first.world_mut().insert_resource("other type".to_string());
    first.tick_once().unwrap();
    assert_eq!(first.world_mut().resource::<u64>(), Some(&11));
    assert_eq!(second.world_mut().resource::<u64>(), Some(&10));
    assert!(second.world_mut().resource::<String>().is_none());
    assert!(second.install_application(Counter).is_err());
    let mut invalid = Runtime::new(RuntimeConfig { tick_rate_hz: 0 }, LocalChannel::connected());
    assert!(invalid.install_application(Counter).is_err());
}

#[test]
fn host_validation_rejects_bad_inputs_without_discarding_accepted_inputs() {
    let mut runtime = runtime();
    assert_eq!(
        runtime.try_enqueue_input(OscBundle::new(), None).unwrap(),
        0
    );
    let mut bad = OscBundle::new();
    bad.push(OscMessage::new("/bad"));
    assert!(runtime.try_enqueue_input(bad, None).is_err());
    let mut bad_move = OscBundle::new();
    bad_move.push(OscMessage::new("/input/move"));
    assert!(runtime.try_enqueue_input(bad_move, None).is_err());
    assert_eq!(
        runtime.try_enqueue_input(OscBundle::new(), None).unwrap(),
        1
    );
    runtime.tick_once().unwrap();
    assert_eq!(runtime.drain_committed_inputs().len(), 2);
}
