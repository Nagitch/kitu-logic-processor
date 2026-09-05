//! Authoritative Arena application, incrementally ported against the C# oracle.
//!
//! Lifecycle, movement, aim, pause and inventory use the reference contract.
//! Combat and progression follow in their respective migration stages.

use std::collections::HashMap;

use kitu_core::{KituError, Result};
use kitu_ecs::EcsWorld;
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_runtime::{ApplicationTick, InputMetadata, RuntimeApplication, RuntimeInput};
use serde::{Deserialize, Serialize};

use crate::DemoRuntime;

pub mod inventory;
use inventory::Inventory;

/// Stage-independent Arena OSC contract version.
pub const SCHEMA_VERSION: u32 = 1;

/// A ground-plane vector; `y` maps to Unity world Z for reference compatibility.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    /// World X component.
    pub x: f32,
    /// World Z component.
    pub y: f32,
}

impl Vec2 {
    fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
    fn normalized(self) -> Self {
        let length = self.length();
        if length > 0.00001 {
            Self {
                x: self.x / length,
                y: self.y / length,
            }
        } else {
            Self::default()
        }
    }
}

/// Detached state shared by the host, Unity and command-line inspectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaState {
    /// Last completed source tick; -1 before the first update.
    pub tick: i64,
    /// Number of gameplay steps, excluding paused/opening/results ticks.
    pub simulation_steps: u64,
    /// Reference ArenaPhase discriminant (opening=0, preparation=1).
    pub phase: i32,
    /// Runtime-owned pause/interaction overlay.
    pub overlay: String,
    /// Current floor; preparation is zero.
    pub floor: i32,
    /// Gameplay elapsed seconds, accumulated using reference f32 arithmetic.
    pub elapsed: f32,
    /// Authoritative player ground-plane position.
    pub player_position: Vec2,
    /// Last valid normalized aim direction.
    pub aim_direction: Vec2,
    /// Complete item ownership and health/shield clock projection.
    pub inventory: Inventory,
    /// Whether the current safe phase offers a chest.
    pub chest_available: bool,
    /// Whether the current phase offers a portal (progression migrates in stage 5).
    pub portal_available: bool,
}

impl Default for ArenaState {
    fn default() -> Self {
        Self {
            tick: -1,
            simulation_steps: 0,
            phase: 0,
            overlay: "none".into(),
            floor: 0,
            elapsed: 0.0,
            player_position: Vec2::default(),
            aim_direction: Vec2 { x: 0.0, y: 1.0 },
            inventory: Inventory::default(),
            chest_available: false,
            portal_available: false,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Controls {
    movement: Vec2,
    has_aim: bool,
    aim: Vec2,
    fire_a: bool,
    fire_b: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Command {
    Start,
    Menu,
    Pause,
    Resume,
    Disconnect,
    Frame(Controls),
    InventoryOpen,
    ChestOpen,
    Close,
    Inventory {
        operation: InventoryOperation,
        item_id: i32,
        index: i32,
        slot: i32,
    },
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum InventoryOperation {
    Take,
    Equip,
    Unequip,
    Discard,
    Upgrade,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Outcome {
    sequence: u64,
    source: String,
    id: u64,
    tick: i64,
    applied_tick: i64,
    accepted: bool,
    duplicate: bool,
    code: String,
}

#[derive(Default)]
struct ArenaSession {
    state: ArenaState,
    controls: Controls,
    seen: HashMap<(String, u64), (OscMessage, Outcome)>,
    high_water: HashMap<String, u64>,
}

struct ArenaApplication;

/// Installs Arena state and behavior in an unstarted demo runtime.
pub fn install(runtime: &mut DemoRuntime) -> Result<()> {
    runtime.install_application(ArenaApplication)?;
    runtime.world_mut().insert_resource(ArenaSession::default());
    Ok(())
}

/// Validates one Arena envelope before a network adapter admits it to the runtime.
///
/// # Errors
/// Rejects missing identity/version, malformed payloads and non-finite coordinates.
pub fn validate_input(message: &OscMessage, metadata: &InputMetadata) -> Result<()> {
    if metadata.schema_version != SCHEMA_VERSION
        || metadata.message_id == 0
        || metadata.source.is_empty()
        || metadata.source.len() > 128
    {
        return Err(KituError::InvalidInput(
            "invalid Arena envelope identity or schema",
        ));
    }
    parse(message).map(|_| ())
}

fn parse(message: &OscMessage) -> Result<Command> {
    let command = match message.address.as_str() {
        "/input/arena/start" => Command::Start,
        "/input/arena/menu" => Command::Menu,
        "/input/arena/pause" => Command::Pause,
        "/input/arena/resume" => Command::Resume,
        "/input/arena/disconnect" => Command::Disconnect,
        "/input/arena/frame" => {
            if let [OscArg::Float(x), OscArg::Float(y), OscArg::Bool(has_aim), OscArg::Float(ax), OscArg::Float(ay), OscArg::Bool(fire_a), OscArg::Bool(fire_b)] =
                message.args.as_slice()
            {
                if [x, y, ax, ay].into_iter().all(|v| v.is_finite()) {
                    return Ok(Command::Frame(Controls {
                        movement: Vec2 { x: *x, y: *y },
                        has_aim: *has_aim,
                        aim: Vec2 { x: *ax, y: *ay },
                        fire_a: *fire_a,
                        fire_b: *fire_b,
                    }));
                }
            }
            return Err(KituError::InvalidInput(
                "Arena frame expects finite move/aim coordinates and boolean controls",
            ));
        }
        "/input/arena/inventory" => Command::InventoryOpen,
        "/input/arena/chest" => Command::ChestOpen,
        "/input/arena/close" => Command::Close,
        "/input/arena/take"
        | "/input/arena/discard"
        | "/input/arena/upgrade"
        | "/input/arena/unequip" => {
            let operation = match message.address.as_str() {
                "/input/arena/take" => InventoryOperation::Take,
                "/input/arena/discard" => InventoryOperation::Discard,
                "/input/arena/upgrade" => InventoryOperation::Upgrade,
                _ => InventoryOperation::Unequip,
            };
            return match message.args.as_slice() {
                [OscArg::Int(item_id), OscArg::Int(target)] => Ok(Command::Inventory {
                    operation,
                    item_id: *item_id,
                    index: if operation == InventoryOperation::Unequip {
                        0
                    } else {
                        *target
                    },
                    slot: if operation == InventoryOperation::Unequip {
                        *target
                    } else {
                        0
                    },
                }),
                _ => Err(KituError::InvalidInput(
                    "Arena operation expects two ordered i32 arguments",
                )),
            };
        }
        "/input/arena/equip" => {
            return match message.args.as_slice() {
                [OscArg::Int(item_id), OscArg::Int(index), OscArg::Int(slot)] => {
                    Ok(Command::Inventory {
                        operation: InventoryOperation::Equip,
                        item_id: *item_id,
                        index: *index,
                        slot: *slot,
                    })
                }
                _ => Err(KituError::InvalidInput(
                    "Arena equip expects three ordered i32 arguments",
                )),
            };
        }
        "/input/arena/use" => {
            return match message.args.as_slice() {
                [OscArg::Int(_)] => Ok(Command::Unsupported),
                _ => Err(KituError::InvalidInput(
                    "Arena use expects an i32 equipment slot",
                )),
            };
        }
        _ => return Err(KituError::InvalidInput("unknown Arena command")),
    };
    if !message.args.is_empty() {
        return Err(KituError::InvalidInput(
            "Arena control expects no payload arguments",
        ));
    }
    Ok(command)
}

impl RuntimeApplication for ArenaApplication {
    fn validate_inputs(&self, inputs: &[RuntimeInput]) -> Result<()> {
        for input in inputs {
            for message in &input.bundle.messages {
                if !message.address.starts_with("/input/arena/") {
                    continue;
                }
                if input.bundle.messages.len() != 1 {
                    return Err(KituError::InvalidInput(
                        "Arena envelopes contain exactly one command",
                    ));
                }
                let metadata = input.metadata.as_ref().ok_or(KituError::InvalidInput(
                    "Arena input requires envelope metadata",
                ))?;
                validate_input(message, metadata)?;
            }
        }
        Ok(())
    }

    fn tick(&mut self, world: &mut EcsWorld, context: ApplicationTick<'_>) -> Vec<OscBundle> {
        let session = world
            .resource_mut::<ArenaSession>()
            .expect("Arena resource installed with application");
        let mut output = OscBundle::new();
        for input in context.inputs {
            for message in &input.bundle.messages {
                if !message.address.starts_with("/input/arena/") {
                    continue;
                }
                let meta = input.metadata.as_ref().expect("validated metadata");
                let command = parse(message).expect("validated command");
                let key = (meta.source.clone(), meta.message_id);
                let tick = context.tick.get() as i64;
                if let Command::Frame(controls) = command {
                    if let Some((_, original)) = session.seen.get(&key) {
                        let mut conflict = original.clone();
                        conflict.tick = tick;
                        conflict.applied_tick = -1;
                        conflict.accepted = false;
                        conflict.duplicate = true;
                        conflict.code = "id_conflict".into();
                        output.push(json_message("/ui/arena/command", &conflict));
                    } else {
                        let previous = session.high_water.entry(meta.source.clone()).or_default();
                        if meta.message_id > *previous {
                            *previous = meta.message_id;
                            if session.state.overlay == "none" {
                                session.controls = controls;
                            }
                        }
                    }
                    continue;
                }
                let outcome = if let Some((original, previous)) = session.seen.get(&key) {
                    let mut outcome = previous.clone();
                    outcome.tick = tick;
                    outcome.duplicate = true;
                    if original != message {
                        outcome.accepted = false;
                        outcome.code = "id_conflict".into();
                        outcome.applied_tick = -1;
                    }
                    outcome
                } else if meta.message_id
                    <= session.high_water.get(&meta.source).copied().unwrap_or(0)
                {
                    // A first-seen discrete ID below the producer high-water mark
                    // may belong to a frame. Never reuse it for a state mutation.
                    Outcome {
                        sequence: input.sequence,
                        source: meta.source.clone(),
                        id: meta.message_id,
                        tick,
                        applied_tick: -1,
                        accepted: false,
                        duplicate: true,
                        code: "id_conflict".into(),
                    }
                } else {
                    session
                        .high_water
                        .insert(meta.source.clone(), meta.message_id);
                    let inventory_request = match &command {
                        Command::Inventory {
                            item_id,
                            index,
                            slot,
                            ..
                        } => Some((*item_id, *index, *slot)),
                        _ => None,
                    };
                    let code = execute(session, command);
                    if let Some((item_id, index, slot)) = inventory_request.filter(|_| code == "ok")
                    {
                        output.push(json_message(
                            "/game/arena/inventory",
                            &serde_json::json!({
                                "tick": tick, "sequence": input.sequence, "source": meta.source,
                                "messageId": meta.message_id, "operation": message.address,
                                "itemId": item_id, "index": index, "slot": slot,
                                "inventory": session.state.inventory,
                            }),
                        ));
                    }
                    let outcome = Outcome {
                        sequence: input.sequence,
                        source: meta.source.clone(),
                        id: meta.message_id,
                        tick,
                        applied_tick: tick,
                        accepted: code == "ok",
                        duplicate: false,
                        code: code.into(),
                    };
                    session.seen.insert(key, (message.clone(), outcome.clone()));
                    outcome
                };
                output.push(json_message("/ui/arena/command", &outcome));
            }
        }
        if session.state.phase == 1 && session.state.overlay == "none" {
            let mut movement = session.controls.movement;
            if movement.x * movement.x + movement.y * movement.y > 1.0 {
                movement = movement.normalized();
            }
            session.state.player_position.x = (session.state.player_position.x
                + movement.x * (5.0 * context.dt))
                .clamp(-9.5, 9.5);
            session.state.player_position.y = (session.state.player_position.y
                + movement.y * (5.0 * context.dt))
                .clamp(-9.5, 9.5);
            if session.controls.has_aim {
                let direction = Vec2 {
                    x: session.controls.aim.x - session.state.player_position.x,
                    y: session.controls.aim.y - session.state.player_position.y,
                };
                if direction.x * direction.x + direction.y * direction.y > 0.000001 {
                    session.state.aim_direction = direction.normalized();
                }
            }
            session.state.inventory.advance_time(context.dt);
            session.state.elapsed += context.dt;
            session.state.simulation_steps += 1;
        }
        session.state.tick = context.tick.get() as i64;
        output.push(json_message("/ui/arena/state", &session.state));
        vec![output]
    }

    fn snapshot(&self, world: &EcsWorld) -> Vec<OscBundle> {
        let session = world
            .resource::<ArenaSession>()
            .expect("Arena resource installed with application");
        let mut output = OscBundle::new();
        output.push(json_message("/ui/arena/state", &session.state));
        vec![output]
    }
}

fn execute(session: &mut ArenaSession, command: Command) -> &'static str {
    match command {
        Command::Start if session.state.phase == 0 || session.state.phase == 5 => {
            let simulation_steps = session.state.simulation_steps;
            let mut inventory = Inventory::default();
            inventory.create_chest(0);
            session.state = ArenaState {
                inventory,
                chest_available: true,
                portal_available: true,
                phase: 1,
                player_position: Vec2 { x: 0.0, y: -7.0 },
                simulation_steps,
                ..ArenaState::default()
            };
        }
        Command::Menu => {
            let simulation_steps = session.state.simulation_steps;
            let inventory = Inventory {
                equipment: std::array::from_fn(|_| inventory::Item::default()),
                ..Inventory::default()
            };
            session.state = ArenaState {
                inventory,
                player_position: Vec2 { x: 0.0, y: -7.0 },
                simulation_steps,
                ..ArenaState::default()
            };
        }
        Command::Pause | Command::Disconnect
            if session.state.phase != 0 && session.state.phase != 5 =>
        {
            session.state.overlay = "pause".into()
        }
        Command::Resume
            if session.state.overlay == "pause"
                && session.state.phase != 0
                && session.state.phase != 5 =>
        {
            session.state.overlay = "none".into()
        }
        Command::InventoryOpen if is_safe(session) && session.state.overlay == "none" => {
            session.state.overlay = "inventory".into()
        }
        Command::ChestOpen if is_safe(session) && session.state.overlay == "none" => {
            let delta = Vec2 {
                x: session.state.player_position.x + 3.0,
                y: session.state.player_position.y,
            };
            if !session.state.chest_available || delta.length() > 2.0 {
                return "out_of_range";
            }
            session.state.overlay = "chest".into();
        }
        Command::Close
            if session.state.overlay == "inventory" || session.state.overlay == "chest" =>
        {
            session.state.overlay = "none".into()
        }
        Command::Inventory {
            operation,
            item_id,
            index,
            slot,
        } => return inventory_command(session, operation, item_id, index, slot),
        Command::Unsupported => return "not_yet_implemented",
        _ => return "invalid_state",
    }
    session.controls = Controls::default();
    "ok"
}

fn is_safe(session: &ArenaSession) -> bool {
    session.state.phase == 1 || session.state.phase == 4
}

fn inventory_command(
    session: &mut ArenaSession,
    operation: InventoryOperation,
    item_id: i32,
    index: i32,
    slot: i32,
) -> &'static str {
    if !is_safe(session)
        || (session.state.overlay != "inventory" && session.state.overlay != "chest")
    {
        return "invalid_state";
    }
    let inventory = &mut session.state.inventory;
    let accepted = match operation {
        InventoryOperation::Take => {
            if session.state.overlay != "chest" {
                return "invalid_state";
            }
            if !(0..3).contains(&index) {
                return "invalid_target";
            }
            let Some(chest_index) = inventory
                .chest
                .iter()
                .position(|item| item.id == item_id && item_id > 0)
            else {
                return "stale_item";
            };
            inventory.take(chest_index as i32, index)
        }
        InventoryOperation::Unequip => {
            if !(0..4).contains(&slot) {
                return "invalid_target";
            }
            if item_id <= 0 || inventory.equipment[slot as usize].id != item_id {
                return "stale_item";
            }
            inventory.unequip(slot)
        }
        _ => {
            if !(0..3).contains(&index) {
                return "invalid_target";
            }
            if item_id <= 0 || inventory.backpack[index as usize].id != item_id {
                return "stale_item";
            }
            match operation {
                InventoryOperation::Equip => inventory.equip(index, slot),
                InventoryOperation::Discard => inventory.discard(index),
                _ => inventory.use_upgrade(index),
            }
        }
    };
    if accepted {
        "ok"
    } else {
        "rule_rejected"
    }
}

fn json_message(address: &str, value: &impl Serialize) -> OscMessage {
    let mut message = OscMessage::new(address);
    message.push_arg(OscArg::Str(
        serde_json::to_string(value).expect("finite serializable Arena state"),
    ));
    message
}
