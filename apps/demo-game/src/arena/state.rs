//! Detached Arena state and actor projections, matching the C# field layout.
use super::{inventory::Inventory, Vec2};
use serde::{Deserialize, Serialize};

/// Authoritative enemy values; enum discriminants match the reference.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Enemy {
    /// Run-local entity ID.
    pub id: i32,
    /// Pursuer=0, shooter=1, heavy=2, boss=3.
    pub kind: i32,
    /// Ground-plane position.
    pub position: Vec2,
    /// Current HP.
    pub health: i32,
    /// Initial scaled HP.
    pub max_health: i32,
    /// Collision radius.
    pub radius: f32,
    /// Movement units per second.
    pub speed: f32,
    /// Scaled attack damage.
    pub damage: i32,
    /// Attack interval in seconds.
    pub attack_interval: f32,
    /// Movement stops at this attack distance.
    pub attack_range: f32,
    /// Remaining seconds before the next attack.
    pub attack_cooldown: f32,
    /// Pursuit=0, telegraph=1, recovery=2.
    pub boss_state: i32,
    /// Remaining boss behavior phase seconds.
    pub phase_remaining: f32,
}

/// A swept projectile with reference-compatible range and radius.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Projectile {
    /// Run-local entity ID.
    pub id: i32,
    /// Current position.
    pub position: Vec2,
    /// Travel direction.
    pub direction: Vec2,
    /// Movement units per second.
    pub speed: f32,
    /// Remaining travel range.
    pub distance_remaining: f32,
    /// Damage fixed at firing.
    pub damage: i32,
    /// Whether the projectile targets the player.
    pub enemy_owned: bool,
    /// Swept collision radius.
    pub radius: f32,
}

impl Default for Projectile {
    fn default() -> Self {
        Self {
            id: 0,
            position: Vec2::default(),
            direction: Vec2::default(),
            speed: 0.0,
            distance_remaining: 0.0,
            damage: 0,
            enemy_owned: false,
            radius: 0.14,
        }
    }
}

/// A grenade with its start, bounded target and remaining flight time.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Grenade {
    /// Run-local entity ID.
    pub id: i32,
    /// Throw origin.
    pub start: Vec2,
    /// Bounded landing point.
    pub target: Vec2,
    /// Current interpolated position.
    pub position: Vec2,
    /// Remaining flight seconds.
    pub remaining: f32,
    /// Scaled damage fixed at throw.
    pub damage: i32,
}

/// A presentation effect whose lifetime follows the game clock.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Effect {
    /// Run-local effect ID.
    pub id: i32,
    /// Slash=0, explosion=1, hit=2.
    pub kind: i32,
    /// Ground-plane origin.
    pub position: Vec2,
    /// Presentation direction.
    pub direction: Vec2,
    /// Visible radius.
    pub radius: f32,
    /// Remaining lifetime seconds.
    pub remaining: f32,
}

/// Values captured at death; later inventory changes cannot rewrite a result.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    /// Whether a death result exists.
    pub present: bool,
    /// Floor reached.
    pub floor: i32,
    /// Enemy kills.
    pub enemies_defeated: i32,
    /// Boss kills.
    pub bosses_defeated: i32,
    /// Cleared floors.
    pub floors_cleared: i32,
    /// HP capacity at death.
    pub max_health: i32,
    /// Run elapsed time.
    pub elapsed: f32,
    /// Damage multiplier at death.
    pub attack_multiplier: f32,
    /// Copied names in equipment slot order, or an empty list before death.
    pub equipment_names: Vec<String>,
}

/// Detached state shared by the host, Unity and command-line inspectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArenaState {
    // Run content is emitted separately with its full values and hash; this
    // projection keeps the frozen C# gameplay field contract unchanged.
    #[serde(skip)]
    pub(super) rules: std::sync::Arc<super::config::ArenaConfig>,
    /// Last completed source tick; -1 before the first update.
    pub tick: i64,
    /// Number of gameplay steps, excluding paused/opening/results ticks.
    pub simulation_steps: u64,
    /// Reference ArenaPhase: opening=0, preparation=1, transition=2, combat=3, cleared=4, results=5.
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
    /// Total enemies defeated in this run.
    pub enemies_defeated: i32,
    /// Total bosses defeated in this run.
    pub bosses_defeated: i32,
    /// Cleared floors in this run.
    pub floors_cleared: i32,
    /// Next run-local entity/effect ID.
    pub next_entity_id: i32,
    /// Remaining transition seconds; transitions do not advance the elapsed run time.
    pub transition_remaining: f32,
    /// Portal must be exited before it can trigger another transition.
    pub portal_armed: bool,
    /// Weapon A/B cooldown seconds.
    pub weapon_cooldowns: [f32; 2],
    /// Enemies in stable spawn order.
    pub enemies: Vec<Enemy>,
    /// Active projectiles in allocation order.
    pub projectiles: Vec<Projectile>,
    /// Active grenades in allocation order.
    pub grenades: Vec<Grenade>,
    /// Active presentation effects in allocation order.
    pub effects: Vec<Effect>,
    /// Detached immutable result values when the player dies.
    pub result: RunResult,
}

impl Default for ArenaState {
    fn default() -> Self {
        Self {
            rules: Default::default(),
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
            enemies_defeated: 0,
            bosses_defeated: 0,
            floors_cleared: 0,
            next_entity_id: 1,
            transition_remaining: 0.0,
            portal_armed: false,
            weapon_cooldowns: [0.0; 2],
            enemies: Vec::new(),
            projectiles: Vec::new(),
            grenades: Vec::new(),
            effects: Vec::new(),
            result: RunResult::default(),
        }
    }
}
