//! Run a synthetic integer and Rapier replay through Kitu's fixed-tick runtime.

use std::{
    env,
    fmt::Write as _,
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
};

use anyhow::{ensure, Context, Result};
use kitu_core::KituError;
use kitu_ecs::{
    spatial::{AabbUm, ColliderRole, DisplacementUm, PositionMm, PositionUm, SpatialWorld},
    EcsWorld,
};
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_runtime::{
    physics::{RigidBodyWorld, RigidBoxKind, RigidBoxMm},
    ApplicationTick, Runtime, RuntimeApplication, RuntimeConfig, RuntimeInput,
};
use kitu_transport::LocalChannel;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const INPUT_ADDRESS: &str = "/determinism/input";
const TICK_RATE_HZ: u32 = 60;
const FLOOR_ID: &str = "floor";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    tick_count: u64,
    start_mm: [i64; 3],
    #[serde(default)]
    actor_half_extents_um: [i64; 3],
    boxes: Vec<BoxMm>,
    rigid_boxes: Vec<RigidBox>,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RigidBox {
    id: String,
    center_mm: [i64; 3],
    #[serde(default)]
    blocks_spatial_queries: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    Wall,
    Target,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoxMm {
    id: String,
    role: Role,
    min_mm: [i64; 3],
    max_mm: [i64; 3],
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
    Move {
        delta_um: [i64; 3],
    },
    HitSegment {
        delta_um: [i64; 3],
    },
    RemoveBox {
        id: String,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    fraction: Option<(u64, u64)>,
}

#[derive(Serialize)]
struct ObstacleTrace<'a> {
    id: &'a str,
    min_um: [i64; 3],
    max_um: [i64; 3],
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    dynamic_obstacles: Vec<ObstacleTrace<'a>>,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    moves_blocked_by: &'a [Option<String>],
}

struct ProbeState {
    position: PositionUm,
    half_extents_um: [i64; 3],
    spatial: SpatialWorld,
    physics: RigidBodyWorld,
    hits: Vec<HitEvent>,
    moves_blocked_by: Vec<Option<String>>,
    failure: Option<anyhow::Error>,
}

impl ProbeState {
    fn new(fixture: &Fixture) -> Result<Self> {
        let position = PositionMm(fixture.start_mm).to_micrometres()?;
        let mut spatial = SpatialWorld::new();
        for shape in &fixture.boxes {
            let bounds = AabbUm::from_mm(PositionMm(shape.min_mm), PositionMm(shape.max_mm))?;
            let role = match shape.role {
                Role::Wall => ColliderRole::Obstacle,
                Role::Target => ColliderRole::Target,
            };
            spatial.insert_static(&shape.id, bounds, role)?;
        }
        spatial.move_aabb(
            position,
            fixture.actor_half_extents_um,
            DisplacementUm([0; 3]),
        )?;

        let mut physics = RigidBodyWorld::new();
        physics.insert_box(
            FLOOR_ID,
            RigidBoxMm {
                center: PositionMm([0, -250, 0]),
                half_extents_mm: [20000, 250, 20000],
                kind: RigidBoxKind::Fixed,
                blocks_spatial_queries: false,
            },
        )?;
        for shape in &fixture.rigid_boxes {
            physics.insert_box(
                &shape.id,
                RigidBoxMm {
                    center: PositionMm(shape.center_mm),
                    half_extents_mm: [250; 3],
                    kind: RigidBoxKind::Dynamic,
                    blocks_spatial_queries: shape.blocks_spatial_queries,
                },
            )?;
        }
        Ok(Self {
            position,
            half_extents_um: fixture.actor_half_extents_um,
            spatial,
            physics,
            hits: Vec::new(),
            moves_blocked_by: Vec::new(),
            failure: None,
        })
    }

    fn advance(&mut self, context: &ApplicationTick<'_>) -> Result<()> {
        self.hits.clear();
        self.moves_blocked_by.clear();
        let mut spatial_actions = Vec::new();
        for message in context
            .inputs
            .iter()
            .flat_map(|input| &input.bundle.messages)
        {
            match decode(message)? {
                Action::Impulse {
                    id,
                    milli_newton_seconds,
                    offset_mm,
                } => {
                    self.physics
                        .apply_impulse(&id, milli_newton_seconds, offset_mm)?;
                }
                Action::RemoveBox { id } => self.physics.remove_box(&id)?,
                action => spatial_actions.push(action),
            }
        }
        self.physics.advance(context, &mut self.spatial)?;
        for action in spatial_actions {
            match action {
                Action::MoveX { dx_um } => self.apply_move([dx_um, 0, 0], false)?,
                Action::Hit { range_um } => self.apply_hit([range_um, 0, 0], Some(range_um))?,
                Action::Move { delta_um } => self.apply_move(delta_um, true)?,
                Action::HitSegment { delta_um } => self.apply_hit(delta_um, None)?,
                Action::Impulse { .. } | Action::RemoveBox { .. } => {
                    unreachable!("rigid action already applied")
                }
            }
        }
        Ok(())
    }

    fn apply_move(&mut self, delta_um: [i64; 3], record_blocker: bool) -> Result<()> {
        let motion = self.spatial.move_aabb(
            self.position,
            self.half_extents_um,
            DisplacementUm(delta_um),
        )?;
        self.position = motion.position;
        if record_blocker {
            self.moves_blocked_by
                .push(motion.obstacle_id.map(str::to_owned));
        }
        Ok(())
    }

    fn apply_hit(&mut self, delta_um: [i64; 3], range_um: Option<i64>) -> Result<()> {
        let hit = self
            .spatial
            .hit_segment(self.position, DisplacementUm(delta_um), None)?;
        let ratio = hit.map(|hit| hit.fraction.ratio());
        self.hits.push(HitEvent {
            id: hit.map(|hit| hit.id.to_owned()),
            distance_um: range_um
                .zip(ratio)
                .map(|(range, (n, d))| (i128::from(range) * i128::from(n) / i128::from(d)) as i64),
            fraction: ratio.filter(|_| range_um.is_none()),
        });
        Ok(())
    }

    fn trace(&self, tick: u64) -> Result<TickTrace<'_>> {
        let frame = self
            .physics
            .last_frame()
            .context("missing completed physics frame")?;
        let bodies = frame
            .poses
            .iter()
            .filter(|pose| pose.id != FLOOR_ID)
            .map(|pose| BodyBits {
                id: &pose.id,
                translation: pose.position_m.map(f32::to_bits),
                rotation: pose.rotation_xyzw.map(f32::to_bits),
                linvel: pose.linear_velocity_m_s.map(f32::to_bits),
                angvel: pose.angular_velocity_rad_s.map(f32::to_bits),
            })
            .collect();
        let dynamic_obstacles = self
            .spatial
            .dynamic_obstacles()?
            .map(|(id, bounds)| ObstacleTrace {
                id,
                min_um: bounds.min().0,
                max_um: bounds.max().0,
            })
            .collect();
        let snapshot = rmp_serde::to_vec(&self.physics.serializable_state())
            .context("serialize Rapier world")?;
        let mut physics_sha256 = String::with_capacity(64);
        for byte in Sha256::digest(snapshot) {
            write!(physics_sha256, "{byte:02x}").expect("writing to String cannot fail");
        }
        Ok(TickTrace {
            tick,
            position_um: self.position.0,
            hits: &self.hits,
            floor_contact: frame
                .poses
                .iter()
                .any(|pose| pose.id == FLOOR_ID && pose.active_contact),
            bodies,
            physics_sha256,
            dynamic_obstacles,
            moves_blocked_by: &self.moves_blocked_by,
        })
    }
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
        if let Err(error) = state.advance(&context) {
            state.failure = Some(error);
        }
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

fn run(fixture: Fixture, mut output: impl Write) -> Result<()> {
    ensure!(fixture.tick_count > 0, "tick_count must be positive");
    let state = ProbeState::new(&fixture)?;
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
            if let Action::Hit { range_um } = action {
                ensure!(*range_um >= 0, "hit range must be nonnegative");
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

    fn trace_fixture(source: &str) -> Vec<Value> {
        let mut bytes = Vec::new();
        run(serde_json::from_str(source).unwrap(), &mut bytes).unwrap();
        std::str::from_utf8(&bytes)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn synthetic_replay_exercises_integer_and_rigid_body_behavior() {
        let rows = trace_fixture(include_str!(
            "../../../../../kitu-integration-runner/scenarios/determinism/ordered-motion.json"
        ));

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
    #[test]
    fn an_obstacle_blocks_movement_and_hits_on_the_tick_it_enters_the_lane() {
        let rows = trace_fixture(include_str!(
            "../../../../../kitu-integration-runner/scenarios/determinism/dynamic-obstacles.json"
        ));
        assert_eq!(rows.len(), 600);
        assert_eq!(rows[0]["hits"][0]["id"], "far_target");
        assert!(
            rows[0]["dynamic_obstacles"][0]["min_um"][1]
                .as_i64()
                .unwrap()
                > 310_000
        );

        assert_eq!(rows[1]["dynamic_obstacles"][0]["id"], "moving_box");
        assert!(
            rows[1]["dynamic_obstacles"][0]["min_um"][1]
                .as_i64()
                .unwrap()
                < 300_000
        );
        assert_eq!(rows[1]["hits"][0]["id"], "moving_box");
        assert_eq!(rows[1]["hits"][0]["fraction"], json!([3, 10]));
        assert_eq!(rows[1]["moves_blocked_by"], json!(["moving_box"]));
        assert_eq!(rows[1]["position_um"], json!([740_000, 300_000, 74_000]));
        assert_eq!(rows[2]["position_um"], rows[1]["position_um"]);

        assert_eq!(rows[3]["bodies"], json!([]));
        assert!(rows[3].get("dynamic_obstacles").is_none());
        assert_eq!(rows[3]["position_um"], json!([1_500_000, 300_000, 150_000]));
        assert_eq!(rows[3]["moves_blocked_by"], json!([null]));
        assert_eq!(rows[3]["hits"][0]["id"], "far_target");
        assert_eq!(rows[599]["position_um"], rows[3]["position_um"]);
    }
}
