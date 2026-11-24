//! Run a synthetic integer and Rapier replay through Kitu's fixed-tick runtime.

mod geometry;

use std::{
    collections::BTreeMap,
    env,
    fmt::Write as _,
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
};

use anyhow::{ensure, Context, Result};
use geometry::{BoxMm, Hit, SpatialState};
use kitu_core::KituError;
use kitu_ecs::EcsWorld;
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_runtime::{ApplicationTick, Runtime, RuntimeApplication, RuntimeConfig, RuntimeInput};
use kitu_transport::LocalChannel;
use rapier3d::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const INPUT_ADDRESS: &str = "/determinism/input";
const TICK_RATE_HZ: u32 = 60;
const DT: f32 = 1.0 / TICK_RATE_HZ as f32;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    tick_count: u64,
    start_mm: [i64; 3],
    boxes: Vec<BoxMm>,
    rigid_boxes: Vec<RigidBox>,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RigidBox {
    id: String,
    center_mm: [i64; 3],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    at_tick: u64,
    actions: Vec<Action>,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    MoveX {
        dx_um: i64,
    },
    Hit {
        range_um: i64,
    },
    Impulse {
        id: String,
        milli_newton_seconds: [i64; 3],
        offset_mm: [i64; 3],
    },
}

#[derive(Serialize)]
struct HitEvent {
    id: Option<String>,
    distance_um: Option<i64>,
}

impl From<Option<Hit<'_>>> for HitEvent {
    fn from(hit: Option<Hit<'_>>) -> Self {
        Self {
            id: hit.as_ref().map(|hit| hit.id.to_owned()),
            distance_um: hit.map(|hit| hit.distance_um),
        }
    }
}

#[derive(Serialize)]
struct BodyBits<'a> {
    id: &'a str,
    translation: [u32; 3],
    rotation: [u32; 4],
    linvel: [u32; 3],
    angvel: [u32; 3],
}

#[derive(Serialize)]
struct TickTrace<'a> {
    tick: u64,
    position_um: [i64; 3],
    hits: &'a [HitEvent],
    floor_contact: bool,
    bodies: Vec<BodyBits<'a>>,
    physics_sha256: String,
}

struct ProbeState {
    spatial: SpatialState,
    physics: PhysicsWorld,
    floor: ColliderHandle,
    body_ids: BTreeMap<String, RigidBodyHandle>,
    hits: Vec<HitEvent>,
    failure: Option<anyhow::Error>,
}

impl ProbeState {
    fn new(fixture: &mut Fixture) -> Result<Self> {
        let spatial = SpatialState::new(fixture.start_mm, std::mem::take(&mut fixture.boxes))?;
        let mut physics = PhysicsWorld::new();
        physics.integration_parameters.dt = DT;
        let floor = physics.insert_collider(
            ColliderBuilder::cuboid(20.0, 0.25, 20.0).translation(milli_to_vector([0, -250, 0])),
            None,
        );
        let mut body_ids = BTreeMap::new();
        // Insert in authored order. The map is used for lookups and sorted trace output.
        for shape in &fixture.rigid_boxes {
            ensure!(
                !shape.id.is_empty() && !body_ids.contains_key(&shape.id),
                "expected a unique nonempty body ID: {:?}",
                shape.id,
            );
            let (handle, _) = physics.insert(
                RigidBodyBuilder::dynamic().translation(milli_to_vector(shape.center_mm)),
                ColliderBuilder::cuboid(0.25, 0.25, 0.25),
            );
            body_ids.insert(shape.id.clone(), handle);
        }
        Ok(Self {
            spatial,
            physics,
            floor,
            body_ids,
            hits: Vec::new(),
            failure: None,
        })
    }

    fn apply(&mut self, action: Action) -> Result<()> {
        match action {
            Action::MoveX { dx_um } => self.spatial.move_x(dx_um)?,
            Action::Hit { range_um } => self.hits.push(self.spatial.hit_x(range_um).into()),
            Action::Impulse {
                id,
                milli_newton_seconds,
                offset_mm,
            } => {
                let body = &mut self.physics.bodies[self.body_ids[&id]];
                let point = body.translation() + milli_to_vector(offset_mm);
                body.apply_impulse_at_point(milli_to_vector(milli_newton_seconds), point, true);
            }
        }
        Ok(())
    }

    fn trace(&self, tick: u64) -> Result<TickTrace<'_>> {
        let mut bodies = Vec::with_capacity(self.body_ids.len());
        for (id, handle) in &self.body_ids {
            let body = &self.physics.bodies[*handle];
            let translation = body.translation().to_array();
            let rotation = body.rotation().to_array();
            let linvel = body.linvel().to_array();
            let angvel = body.angvel().to_array();
            ensure!(
                translation
                    .into_iter()
                    .chain(rotation)
                    .chain(linvel)
                    .chain(angvel)
                    .all(f32::is_finite),
                "non-finite rigid body state at tick {tick} for {id}",
            );
            bodies.push(BodyBits {
                id,
                translation: translation.map(f32::to_bits),
                rotation: rotation.map(f32::to_bits),
                linvel: linvel.map(f32::to_bits),
                angvel: angvel.map(f32::to_bits),
            });
        }
        let snapshot = rmp_serde::to_vec(&self.physics).context("serialize Rapier world")?;
        let mut physics_sha256 = String::with_capacity(64);
        for byte in Sha256::digest(snapshot) {
            write!(physics_sha256, "{byte:02x}").expect("writing to String cannot fail");
        }
        Ok(TickTrace {
            tick,
            position_um: self.spatial.position_um,
            hits: &self.hits,
            floor_contact: self
                .physics
                .contact_pairs_with(self.floor)
                .any(|pair| pair.has_any_active_contact()),
            bodies,
            physics_sha256,
        })
    }
}

fn milli_to_vector(values: [i64; 3]) -> Vector {
    Vector::from_array(values.map(|value| (value as f64 / 1000.0) as f32))
}

struct ProbeApplication;

impl RuntimeApplication for ProbeApplication {
    fn validate_inputs(&self, inputs: &[RuntimeInput]) -> kitu_core::Result<()> {
        for message in inputs.iter().flat_map(|input| &input.bundle.messages) {
            decode(message)?;
        }
        Ok(())
    }

    fn tick(&mut self, world: &mut EcsWorld, context: ApplicationTick<'_>) -> Vec<OscBundle> {
        let state = world
            .resource_mut::<ProbeState>()
            .expect("probe state installed");
        state.hits.clear();
        for message in context
            .inputs
            .iter()
            .flat_map(|input| &input.bundle.messages)
        {
            let action = decode(message).expect("input validated before tick");
            if let Err(error) = state.apply(action) {
                state.failure = Some(error);
                return Vec::new();
            }
        }
        state.physics.step();
        Vec::new()
    }

    fn snapshot(&self, _world: &EcsWorld) -> Vec<OscBundle> {
        Vec::new()
    }
}

fn decode(message: &OscMessage) -> kitu_core::Result<Action> {
    match (message.address.as_str(), message.args.as_slice()) {
        (INPUT_ADDRESS, [OscArg::Str(payload)]) => serde_json::from_str(payload)
            .map_err(|_| KituError::InvalidInput("malformed determinism input")),
        _ => Err(KituError::InvalidInput("unexpected determinism input")),
    }
}

fn run(mut fixture: Fixture, mut output: impl Write) -> Result<()> {
    ensure!(fixture.tick_count > 0, "tick_count must be positive");
    let state = ProbeState::new(&mut fixture)?;
    let mut previous_tick = None;
    for step in &fixture.steps {
        ensure!(
            step.at_tick < fixture.tick_count,
            "action outside the replay"
        );
        ensure!(
            previous_tick.is_none_or(|previous| previous <= step.at_tick),
            "steps must be in tick order"
        );
        previous_tick = Some(step.at_tick);
        for action in &step.actions {
            match action {
                Action::Hit { range_um } => {
                    ensure!(*range_um >= 0, "hit range must be nonnegative")
                }
                Action::Impulse { id, .. } => {
                    ensure!(state.body_ids.contains_key(id), "unknown body {id}")
                }
                Action::MoveX { .. } => {}
            }
        }
    }

    let mut runtime = Runtime::new(
        RuntimeConfig {
            tick_rate_hz: TICK_RATE_HZ,
        },
        LocalChannel::connected(),
    );
    runtime.world_mut().insert_resource(state);
    runtime.install_application(ProbeApplication)?;
    let mut steps = fixture.steps.into_iter().peekable();
    for _ in 0..fixture.tick_count {
        let tick = runtime.current_tick().get();
        while steps.peek().is_some_and(|step| step.at_tick == tick) {
            let mut bundle = OscBundle::new();
            for action in steps.next().unwrap().actions {
                let mut message = OscMessage::new(INPUT_ADDRESS);
                message.push_arg(OscArg::Str(serde_json::to_string(&action)?));
                bundle.push(message);
            }
            runtime.enqueue_input(bundle);
        }

        runtime.tick_once()?;
        runtime.drain_committed_inputs();
        let state = runtime
            .world_mut()
            .resource_mut::<ProbeState>()
            .expect("probe state installed");
        if let Some(error) = state.failure.take() {
            return Err(error).with_context(|| format!("apply input at tick {tick}"));
        }
        serde_json::to_writer(&mut output, &state.trace(tick)?)?;
        output.write_all(b"\n")?;
    }
    output.flush()?;
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = env::args_os().skip(1).map(PathBuf::from).collect();
    let [scenario, trace] = args.as_slice() else {
        anyhow::bail!("usage: kitu-determinism-probe <scenario.json> <trace.jsonl>");
    };
    let fixture = fs::read(scenario).with_context(|| format!("read {}", scenario.display()))?;
    let fixture = serde_json::from_slice(&fixture).context("parse determinism fixture")?;
    let output = File::create(trace).with_context(|| format!("create {}", trace.display()))?;
    run(fixture, BufWriter::new(output))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn synthetic_replay_exercises_integer_and_rigid_body_behavior() {
        let source = include_str!(
            "../../../../../kitu-integration-runner/scenarios/determinism/ordered-motion.json"
        );
        let mut bytes = Vec::new();
        run(serde_json::from_str(source).unwrap(), &mut bytes).unwrap();
        let rows: Vec<Value> = std::str::from_utf8(&bytes)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();

        assert_eq!(rows.len(), 600);
        for (tick, row) in rows.iter().enumerate() {
            assert_eq!(row["tick"], tick);
        }
        assert_eq!(rows[0]["position_um"], json!([150001, 0, 0]));
        assert_eq!(
            rows[0]["hits"],
            json!([
                {"id": "near_target", "distance_um": 75000},
                {"id": "wall", "distance_um": 149999}
            ])
        );
        assert_eq!(rows[1]["position_um"], rows[0]["position_um"]);
        assert_eq!(rows[1]["hits"], json!([]));
        assert_eq!(rows[2]["position_um"], json!([300000, 0, 0]));
        assert_eq!(rows[2]["hits"], json!([{"id":"wall","distance_um":0}]));
        assert_eq!(rows[3]["position_um"], json!([199999, 0, 0]));
        assert_eq!(rows[599]["position_um"], rows[3]["position_um"]);

        assert_eq!(rows[0]["bodies"][0]["id"], "box_a");
        assert_eq!(rows[0]["floor_contact"], false);
        assert!(rows[1..240].iter().any(|row| row["floor_contact"] == true));
        assert_ne!(rows[0]["physics_sha256"], rows[1]["physics_sha256"]);
        assert_ne!(
            rows[239]["bodies"][0]["rotation"],
            rows[240]["bodies"][0]["rotation"]
        );
    }
}
