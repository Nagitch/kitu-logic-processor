//! Select one application spatial mode at startup using the same binary.

use kitu_core::{KituError, Result};
use kitu_ecs::{
    spatial::{DisplacementUm, PositionMm, PositionUm, SpatialWorld},
    EcsWorld,
};
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_runtime::{
    build_runtime,
    physics::{RigidBodyWorld, RigidBoxKind, RigidBoxMm},
    ApplicationTick, RuntimeApplication, RuntimeInput,
};
use kitu_transport::LocalChannel;

const INTEGER_MOVE: &str = "/example/move_um";

#[derive(Clone, Copy)]
enum Mode {
    Legacy,
    Deterministic,
}

struct SpatialSession {
    position: PositionUm,
    spatial: SpatialWorld,
    physics: RigidBodyWorld,
    failure: Option<KituError>,
}

impl SpatialSession {
    fn new() -> Result<Self> {
        let mut physics = RigidBodyWorld::new();
        physics.insert_box(
            "obstacle",
            RigidBoxMm {
                center: PositionMm([1250, 0, 0]),
                half_extents_mm: [250; 3],
                kind: RigidBoxKind::Fixed,
                blocks_spatial_queries: true,
            },
        )?;
        Ok(Self {
            position: PositionMm([0; 3]).to_micrometres()?,
            spatial: SpatialWorld::new(),
            physics,
            failure: None,
        })
    }

    fn advance(&mut self, context: &ApplicationTick<'_>) -> Result<()> {
        self.physics.advance(context, &mut self.spatial)?;
        for message in context
            .inputs
            .iter()
            .flat_map(|input| &input.bundle.messages)
        {
            let delta = parse_move(message)?;
            self.position = self
                .spatial
                .move_aabb(self.position, [0; 3], delta)?
                .position;
        }
        Ok(())
    }
}

struct SpatialApplication;

impl RuntimeApplication for SpatialApplication {
    fn validate_inputs(&self, inputs: &[RuntimeInput]) -> Result<()> {
        for message in inputs.iter().flat_map(|input| &input.bundle.messages) {
            parse_move(message)?;
        }
        Ok(())
    }

    fn tick(&mut self, world: &mut EcsWorld, context: ApplicationTick<'_>) -> Vec<OscBundle> {
        let session = world
            .resource_mut::<SpatialSession>()
            .expect("session installed at startup");
        session.failure = session.advance(&context).err();
        Vec::new()
    }

    fn snapshot(&self, _world: &EcsWorld) -> Vec<OscBundle> {
        Vec::new()
    }
}

fn parse_move(message: &OscMessage) -> Result<DisplacementUm> {
    match (message.address.as_str(), message.args.as_slice()) {
        (INTEGER_MOVE, [OscArg::Int64(x), OscArg::Int64(y), OscArg::Int64(z)]) => {
            Ok(DisplacementUm([*x, *y, *z]))
        }
        _ => Err(KituError::InvalidInput("expected an integer move")),
    }
}

fn run(mode: Mode) -> Result<f64> {
    let mut runtime = build_runtime(LocalChannel::connected());
    let message = match mode {
        Mode::Legacy => {
            let mut message = OscMessage::new("/input/move");
            message.push_arg(OscArg::Str("actor".into()));
            message.push_arg(OscArg::Float(2.0));
            message.push_arg(OscArg::Float(0.0));
            message
        }
        Mode::Deterministic => {
            runtime.world_mut().insert_resource(SpatialSession::new()?);
            runtime.install_application(SpatialApplication)?;
            let mut message = OscMessage::new(INTEGER_MOVE);
            message.push_arg(OscArg::Int64(2_000_000));
            message.push_arg(OscArg::Int64(0));
            message.push_arg(OscArg::Int64(0));
            message
        }
    };
    let mut input = OscBundle::new();
    input.push(message);
    runtime.try_enqueue_input(input, None)?;
    runtime.tick_once()?;

    match mode {
        Mode::Legacy => Ok(f64::from(
            runtime
                .world()
                .world_object("actor")
                .expect("legacy move committed")
                .transform
                .x,
        )),
        Mode::Deterministic => {
            let session = runtime
                .world_mut()
                .resource_mut::<SpatialSession>()
                .expect("session installed");
            if let Some(error) = session.failure.take() {
                return Err(error);
            }
            Ok(session.position.to_metres()[0])
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mode = match args.as_slice() {
        [mode] if mode == "legacy" => Mode::Legacy,
        [mode] if mode == "deterministic" => Mode::Deterministic,
        _ => {
            return Err(KituError::InvalidInput(
                "usage: spatial_session <legacy|deterministic>",
            ))
        }
    };
    let x_m = run(mode)?;
    println!("mode={} x_m={x_m}", args[0]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_session_modes_run_in_one_build() {
        assert_eq!(run(Mode::Legacy).unwrap(), 2.0);
        assert_eq!(run(Mode::Deterministic).unwrap(), 1.0);
    }
}
