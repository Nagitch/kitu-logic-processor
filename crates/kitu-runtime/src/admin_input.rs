//! Queued world inspection actions for live tooling; legacy direct APIs remain available.
use super::*;

enum Command {
    Spawn(String, f32, f32, f32),
    Move(String, f32, f32, f32),
    Reset,
}
fn parse(message: &OscMessage) -> Result<Command> {
    let invalid = KituError::InvalidInput(
        "world action expects an ID/kind and three finite floats, or reset with no arguments",
    );
    match (message.address.as_str(), message.args.as_slice()) {
        ("/admin/world/reset", []) => Ok(Command::Reset),
        (address, [OscArg::Str(id), OscArg::Float(x), OscArg::Float(y), OscArg::Float(z)])
            if !id.is_empty() && id.len() <= 128 && [x, y, z].iter().all(|v| v.is_finite()) =>
        {
            match address {
                "/admin/world/spawn" => Ok(Command::Spawn(id.clone(), *x, *y, *z)),
                "/admin/world/move" => Ok(Command::Move(id.clone(), *x, *y, *z)),
                _ => Err(invalid),
            }
        }
        _ => Err(invalid),
    }
}
pub(super) fn validate(message: &OscMessage) -> Result<()> {
    if message.address.starts_with("/admin/world/") {
        parse(message)?;
    }
    Ok(())
}
impl<T: Transport> Runtime<T> {
    fn emit_world_transform(&mut self, object: WorldObject) -> Result<String> {
        // The command is already a recorded input. Do not synthesize another
        // input here: replay would regenerate it as well as replay its capture.
        let transform = PlayerTransform {
            x: object.transform.x,
            y: object.transform.z,
            z: 0.0,
        };
        self.queue_output(render_player_transform_message(
            self.tick, &object.id, &transform,
        )?);
        self.player_transforms.insert(object.id.clone(), transform);
        Ok(object.id)
    }
    pub(super) fn apply_queued_world_actions(&mut self, inputs: &[RuntimeInput]) {
        for input in inputs {
            for message in input
                .bundle
                .messages
                .iter()
                .filter(|m| m.address.starts_with("/admin/world/"))
            {
                let result = parse(message).and_then(|command| match command {
                    Command::Spawn(kind, x, y, z) => self
                        .world
                        .spawn_world_object(kind, WorldTransform::new(x, y, z))
                        .and_then(|object| self.emit_world_transform(object)),
                    Command::Move(id, x, y, z) => self
                        .world
                        .move_world_object(&id, WorldTransform::new(x, y, z))
                        .and_then(|object| self.emit_world_transform(object)),
                    Command::Reset => {
                        self.world.reset_world_objects();
                        self.player_transforms.clear();
                        Ok("reset".into())
                    }
                });
                let mut receipt = OscMessage::new("/ui/kitu/command");
                receipt.push_arg(OscArg::Int64(input.sequence as i64));
                receipt.push_arg(OscArg::Int64(self.tick.get() as i64));
                receipt.push_arg(OscArg::Bool(result.is_ok()));
                receipt.push_arg(OscArg::Str(match result {
                    Ok(id) => id,
                    Err(e) => e.to_string(),
                }));
                let mut bundle = OscBundle::new();
                bundle.push(receipt);
                self.queue_output(bundle);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use kitu_transport::LocalChannel;
    #[test]
    fn queued_world_actions_apply_in_order_and_reject_missing_objects_without_losing_tick() {
        let mut runtime = build_runtime(LocalChannel::connected());
        let make = |address: &str, id: &str| {
            let mut m = OscMessage::new(address);
            m.args = vec![
                OscArg::Str(id.into()),
                OscArg::Float(1.),
                OscArg::Float(2.),
                OscArg::Float(3.),
            ];
            let mut b = OscBundle::new();
            b.push(m);
            b
        };
        runtime
            .try_enqueue_input(make("/admin/world/spawn", "marker"), None)
            .unwrap();
        runtime
            .try_enqueue_input(make("/admin/world/move", "missing"), None)
            .unwrap();
        assert!(runtime.inspect_world_state().objects.is_empty());
        runtime.tick_once().unwrap();
        assert_eq!(runtime.current_tick().get(), 1);
        assert_eq!(runtime.inspect_world_state().objects.len(), 1);
        let output = runtime.drain_output_buffer();
        let receipts: Vec<_> = output
            .iter()
            .flat_map(|b| &b.messages)
            .filter(|m| m.address == "/ui/kitu/command")
            .collect();
        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[0].args[2], OscArg::Bool(true));
        assert_eq!(receipts[1].args[2], OscArg::Bool(false));
    }
}
