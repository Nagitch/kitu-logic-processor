//! Opt-in Rapier simulation using the runtime's fixed application ticks.

use std::collections::BTreeMap;

use kitu_core::{KituError, Result, Tick};
use kitu_ecs::spatial::{AabbUm, PositionMm, PositionUm, SpatialWorld};
use rapier3d::prelude::*;

use crate::ApplicationTick;

/// The type of a box in the rigid-body world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigidBoxKind {
    /// An immovable collider.
    Fixed,
    /// A rigid body integrated by Rapier.
    Dynamic,
}

/// A box with identity starting rotation, authored in millimetres.
#[derive(Debug, Clone, Copy)]
pub struct RigidBoxMm {
    /// Starting position of the box centre.
    pub center: PositionMm,
    /// Positive XYZ half extents in millimetres.
    pub half_extents_mm: [i64; 3],
    /// Selects a fixed or dynamic box.
    pub kind: RigidBoxKind,
    /// Publishes the current AABB to the integer spatial world after each step.
    pub blocks_spatial_queries: bool,
}

/// Pose and velocity of one box after a physics step.
#[derive(Debug, Clone)]
pub struct RigidPose {
    /// Stable application-assigned ID.
    pub id: String,
    /// World position in metres.
    pub position_m: [f32; 3],
    /// Quaternion components in XYZW order.
    pub rotation_xyzw: [f32; 4],
    /// Linear velocity in metres per second.
    pub linear_velocity_m_s: [f32; 3],
    /// Angular velocity in radians per second.
    pub angular_velocity_rad_s: [f32; 3],
    /// Whether the box is in an active solver contact.
    pub active_contact: bool,
}

/// Completed physics step. Poses are stored in stable ID order.
#[derive(Debug, Clone)]
pub struct PhysicsFrame {
    /// Source tick of the completed step.
    pub tick: Tick,
    /// Poses of all registered boxes.
    pub poses: Vec<RigidPose>,
}

struct RegisteredBox {
    collider: ColliderHandle,
    body: Option<RigidBodyHandle>,
    blocks_spatial_queries: bool,
}

/// Application-owned rigid-body state in metres with deterministic Rapier settings.
///
/// Apply inputs in a reproducible order and call [Self::advance] inside the
/// application tick, before invoking integer movement and segment queries.
pub struct RigidBodyWorld {
    world: PhysicsWorld,
    boxes: BTreeMap<String, RegisteredBox>,
    step_seconds: Option<f32>,
    last_frame: Option<PhysicsFrame>,
    faulted: bool,
}

impl Default for RigidBodyWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl RigidBodyWorld {
    /// Creates an empty world using Rapier's default gravity.
    pub fn new() -> Self {
        Self {
            world: PhysicsWorld::new(),
            boxes: BTreeMap::new(),
            step_seconds: None,
            last_frame: None,
            faulted: false,
        }
    }

    /// Inserts a box in call order under a unique nonempty public ID.
    ///
    /// # Examples
    ///
    /// ```
    /// use kitu_ecs::spatial::PositionMm;
    /// use kitu_runtime::physics::{RigidBodyWorld, RigidBoxKind, RigidBoxMm};
    ///
    /// let mut world = RigidBodyWorld::new();
    /// world.insert_box("box", RigidBoxMm {
    ///     center: PositionMm([0, 1500, 0]),
    ///     half_extents_mm: [250; 3],
    ///     kind: RigidBoxKind::Dynamic,
    ///     blocks_spatial_queries: true,
    /// }).unwrap();
    /// ```
    pub fn insert_box(&mut self, id: impl Into<String>, shape: RigidBoxMm) -> Result<()> {
        self.ensure_ready()?;
        let id = id.into();
        if id.is_empty() || self.boxes.contains_key(&id) {
            return Err(KituError::InvalidInput(
                "rigid box ID must be nonempty and unique",
            ));
        }
        validate_box(shape)?;
        let center = milli_to_vector(shape.center.0);
        let [x, y, z] = milli_to_vector(shape.half_extents_mm).to_array();
        let shape_builder = ColliderBuilder::cuboid(x, y, z);
        let (body, collider) = match shape.kind {
            RigidBoxKind::Fixed => (
                None,
                self.world
                    .insert_collider(shape_builder.translation(center), None),
            ),
            RigidBoxKind::Dynamic => {
                let (body, collider) = self.world.insert(
                    RigidBodyBuilder::dynamic().translation(center),
                    shape_builder,
                );
                (Some(body), collider)
            }
        };
        self.boxes.insert(
            id,
            RegisteredBox {
                collider,
                body,
                blocks_spatial_queries: shape.blocks_spatial_queries,
            },
        );
        Ok(())
    }

    /// Removes a box. Its obstacle disappears on the next completed step.
    pub fn remove_box(&mut self, id: &str) -> Result<()> {
        self.ensure_ready()?;
        let registration = self
            .boxes
            .remove(id)
            .ok_or(KituError::InvalidInput("unknown rigid box"))?;
        if let Some(body) = registration.body {
            self.world.remove_body(body);
        } else {
            self.world.remove_collider(registration.collider);
        }
        Ok(())
    }

    /// Applies an impulse to a dynamic box, at a world-axis offset from its centre.
    pub fn apply_impulse(
        &mut self,
        id: &str,
        impulse_milli_newton_seconds: [i64; 3],
        offset_mm: [i64; 3],
    ) -> Result<()> {
        self.ensure_ready()?;
        let handle = self
            .boxes
            .get(id)
            .ok_or(KituError::InvalidInput("unknown rigid box"))?
            .body
            .ok_or(KituError::InvalidInput("impulse requires a dynamic box"))?;
        let body = &mut self.world.bodies[handle];
        let point = body.translation() + milli_to_vector(offset_mm);
        body.apply_impulse_at_point(milli_to_vector(impulse_milli_newton_seconds), point, true);
        Ok(())
    }

    /// Advances a runtime tick and atomically replaces the integer obstacle snapshot.
    ///
    /// Repeated or decreasing ticks and changes to the fixed timestep are errors.
    /// The application can skip paused ticks. A projection error invalidates spatial
    /// queries and requires rebuilding both worlds. The last completed frame remains
    /// available for inspection.
    pub fn advance(
        &mut self,
        tick: &ApplicationTick<'_>,
        spatial: &mut SpatialWorld,
    ) -> Result<&PhysicsFrame> {
        self.ensure_ready()?;
        if self
            .last_frame
            .as_ref()
            .is_some_and(|frame| tick.tick <= frame.tick)
        {
            return Err(KituError::InvalidInput("rigid physics ticks must increase"));
        }
        if !tick.dt.is_finite()
            || tick.dt <= 0.0
            || self.step_seconds.is_some_and(|dt| dt != tick.dt)
        {
            return Err(KituError::InvalidInput(
                "rigid physics needs a constant positive timestep",
            ));
        }
        self.step_seconds = Some(tick.dt);
        self.world.integration_parameters.dt = tick.dt;
        self.world.step();
        match self.project(tick.tick, spatial) {
            Ok(frame) => Ok(self.last_frame.insert(frame)),
            Err(error) => {
                self.faulted = true;
                spatial.invalidate_dynamic_obstacles();
                Err(error)
            }
        }
    }

    /// Returns the last completed frame without advancing the simulation.
    pub fn last_frame(&self) -> Option<&PhysicsFrame> {
        self.last_frame.as_ref()
    }

    /// Borrows the engine state for diagnostics or replay comparison.
    ///
    /// The serialization is tied to the Rapier version and the caller's codec.
    pub fn serializable_state(&self) -> impl serde::Serialize + '_ {
        &self.world
    }

    fn ensure_ready(&self) -> Result<()> {
        if self.faulted {
            return Err(KituError::InvalidInput("rigid physics is faulted"));
        }
        Ok(())
    }

    fn project(&self, tick: Tick, spatial: &mut SpatialWorld) -> Result<PhysicsFrame> {
        let mut poses = Vec::with_capacity(self.boxes.len());
        let mut obstacles = Vec::new();
        for (id, entry) in &self.boxes {
            let collider = &self.world.colliders[entry.collider];
            let position_m = collider.translation().to_array();
            let rotation_xyzw = collider.rotation().to_array();
            let (linear_velocity_m_s, angular_velocity_rad_s) =
                entry.body.map_or(([0.0; 3], [0.0; 3]), |handle| {
                    let body = &self.world.bodies[handle];
                    (body.linvel().to_array(), body.angvel().to_array())
                });
            if !position_m
                .into_iter()
                .chain(rotation_xyzw)
                .chain(linear_velocity_m_s)
                .chain(angular_velocity_rad_s)
                .all(f32::is_finite)
            {
                return Err(KituError::InvalidInput("non-finite rigid pose"));
            }
            let active_contact = self
                .world
                .contact_pairs_with(entry.collider)
                .any(|pair| pair.has_any_active_contact());
            poses.push(RigidPose {
                id: id.clone(),
                position_m,
                rotation_xyzw,
                linear_velocity_m_s,
                angular_velocity_rad_s,
                active_contact,
            });
            if entry.blocks_spatial_queries {
                let bounds = collider.compute_aabb();
                let min = quantize_corner(bounds.mins.to_array(), f64::floor)?;
                let max = quantize_corner(bounds.maxs.to_array(), f64::ceil)?;
                obstacles.push((id.clone(), AabbUm::new(min, max)?));
            }
        }
        spatial.replace_dynamic_obstacles(obstacles)?;
        Ok(PhysicsFrame { tick, poses })
    }
}

fn validate_box(shape: RigidBoxMm) -> Result<()> {
    const INVALID_BOUNDS: KituError = KituError::InvalidInput("invalid rigid box bounds");
    for (center, half_extent) in shape.center.0.into_iter().zip(shape.half_extents_mm) {
        if half_extent <= 0 {
            return Err(INVALID_BOUNDS);
        }
        let min = center.checked_sub(half_extent).ok_or(INVALID_BOUNDS)?;
        let max = center.checked_add(half_extent).ok_or(INVALID_BOUNDS)?;
        min.checked_mul(1000).ok_or(INVALID_BOUNDS)?;
        max.checked_mul(1000).ok_or(INVALID_BOUNDS)?;
    }
    Ok(())
}

fn milli_to_vector(values: [i64; 3]) -> Vector {
    Vector::from_array(values.map(|value| (value as f64 / 1000.0) as f32))
}

fn quantize_corner(metres: [f32; 3], round: fn(f64) -> f64) -> Result<PositionUm> {
    let limit = -(i64::MIN as f64);
    let mut micrometres = [0; 3];
    for (dest, source) in micrometres.iter_mut().zip(metres) {
        let value = round(f64::from(source) * 1_000_000.0);
        if !(-limit..limit).contains(&value) {
            return Err(KituError::InvalidInput(
                "rigid obstacle is outside the integer coordinate range",
            ));
        }
        *dest = value as i64;
    }
    Ok(PositionUm(micrometres))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kitu_ecs::spatial::{ColliderRole, DisplacementUm};

    const DT: f32 = 1.0 / 60.0;

    fn tick(number: u64, dt: f32) -> ApplicationTick<'static> {
        ApplicationTick {
            tick: Tick::start().advance_by(number),
            dt,
            inputs: &[],
        }
    }

    fn moving_box(center: [i64; 3]) -> RigidBoxMm {
        RigidBoxMm {
            center: PositionMm(center),
            half_extents_mm: [250; 3],
            kind: RigidBoxKind::Dynamic,
            blocks_spatial_queries: true,
        }
    }

    #[test]
    fn accepts_a_paused_gap_and_rejects_repeated_ticks_or_a_new_step_size() {
        let mut physics = RigidBodyWorld::new();
        let mut spatial = SpatialWorld::new();
        physics
            .insert_box("moving", moving_box([0, 1000, 0]))
            .unwrap();
        let first = physics.advance(&tick(0, DT), &mut spatial).unwrap().poses[0].position_m;
        assert!(physics.advance(&tick(0, DT), &mut spatial).is_err());
        assert!(physics.advance(&tick(1, DT * 2.0), &mut spatial).is_err());
        assert_eq!(physics.last_frame().unwrap().poses[0].position_m, first);
        let second = physics.advance(&tick(5, DT), &mut spatial).unwrap();
        assert_eq!(second.tick.get(), 5);
        assert!(second.poses[0].position_m[1] < first[1]);
        assert!(physics.advance(&tick(4, DT), &mut spatial).is_err());
    }

    #[test]
    fn projection_failure_disables_queries_until_the_worlds_are_rebuilt() {
        let mut physics = RigidBodyWorld::new();
        let mut spatial = SpatialWorld::new();
        physics.insert_box("old", moving_box([0, 1000, 0])).unwrap();
        physics.advance(&tick(0, DT), &mut spatial).unwrap();

        let bounds = AabbUm::from_mm(PositionMm([10, 0, 0]), PositionMm([20, 5, 5])).unwrap();
        spatial
            .insert_static("duplicate", bounds, ColliderRole::Obstacle)
            .unwrap();
        physics
            .insert_box("duplicate", moving_box([1000, 1000, 0]))
            .unwrap();
        assert!(physics.advance(&tick(1, DT), &mut spatial).is_err());
        assert!(spatial.dynamic_obstacles().is_err());
        assert!(spatial
            .hit_segment(PositionUm([0; 3]), DisplacementUm([100; 3]), None)
            .is_err());
        assert_eq!(physics.last_frame().unwrap().tick.get(), 0);
        assert!(physics.remove_box("duplicate").is_err());
        assert!(physics.advance(&tick(2, DT), &mut spatial).is_err());
    }

    #[test]
    fn corners_round_outward_and_reject_magnitudes_outside_i64() {
        let coordinates = [-0.1, 0.1, 0.125];
        assert_eq!(
            quantize_corner(coordinates, f64::floor).unwrap().0,
            [-100001, 100000, 125000]
        );
        assert_eq!(
            quantize_corner(coordinates, f64::ceil).unwrap().0,
            [-100000, 100001, 125000]
        );
        assert!(quantize_corner([f32::MAX, 0.0, 0.0], f64::ceil).is_err());
        assert!(quantize_corner([f32::NAN, 0.0, 0.0], f64::floor).is_err());
    }

    #[test]
    fn projected_bounds_include_the_updated_rotation() {
        let mut physics = RigidBodyWorld::new();
        let mut spatial = SpatialWorld::new();
        physics
            .insert_box("rotor", moving_box([1000, 1000, 0]))
            .unwrap();
        physics.advance(&tick(0, DT), &mut spatial).unwrap();
        let previous = spatial.dynamic_obstacles().unwrap().next().unwrap().1;

        physics
            .apply_impulse("rotor", [100, 0, 0], [0, 250, 0])
            .unwrap();
        let frame = physics.advance(&tick(1, DT), &mut spatial).unwrap();
        assert_ne!(frame.poses[0].rotation_xyzw, [0.0, 0.0, 0.0, 1.0]);
        let current = spatial.dynamic_obstacles().unwrap().next().unwrap().1;
        assert!(
            current.max().0[0] - current.min().0[0] > previous.max().0[0] - previous.min().0[0]
        );
    }
}
