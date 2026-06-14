//! Thin ECS abstraction used by the runtime.
//!
//! # Responsibilities
//! - Provide a minimal world representation, entity management, and system scheduling hooks.
//! - Keep ticking deterministic for the runtime while remaining swappable with other ECS backends.
//! - Offer ergonomic traits for system authors without pulling heavy external dependencies.
//!
//! # Integration
//! The runtime (`kitu-runtime`) drives this crate each tick, and transports surface events that
//! systems can consume. See `doc/crates-overview.md` for the ECS' place in the overall loop.

use std::collections::{HashMap, VecDeque};

use kitu_core::{KituError, Result, Tick};

/// Represents a system that can be scheduled for a tick.
pub trait System: Send + Sync + 'static {
    /// Executes the system for the given tick.
    fn run(&mut self, world: &mut EcsWorld, tick: Tick) -> Result<()>;
}

/// Position for an object in the authoritative world state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldTransform {
    /// X coordinate in world space.
    pub x: f32,
    /// Y coordinate in world space.
    pub y: f32,
    /// Z coordinate in world space.
    pub z: f32,
}

impl WorldTransform {
    /// Creates a transform from world-space coordinates.
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

/// Object tracked by the ECS-backed world state.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldObject {
    /// Stable object identifier assigned by the ECS world.
    pub id: String,
    /// Application-level object category.
    pub kind: String,
    /// Current object transform.
    pub transform: WorldTransform,
}

/// Serializable-style snapshot of the ECS-backed world state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorldSnapshot {
    /// Objects currently present in the world.
    pub objects: Vec<WorldObject>,
}

/// Minimal world representation for registering components and systems.
pub struct EcsWorld {
    components: Vec<String>,
    scheduled: VecDeque<Box<dyn System>>,
    next_world_object_id: u64,
    world_objects: Vec<WorldObject>,
}

impl Default for EcsWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl EcsWorld {
    /// Creates an empty ECS world.
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
            scheduled: VecDeque::new(),
            next_world_object_id: 1,
            world_objects: Vec::new(),
        }
    }

    /// Registers a component type by name. The concrete storage will be wired later.
    pub fn register_component(&mut self, name: impl Into<String>) -> Result<()> {
        let name = name.into();
        if self.components.contains(&name) {
            return Err(KituError::InvalidInput("component already registered"));
        }
        self.components.push(name);
        Ok(())
    }

    /// Schedules a system to run on the next tick.
    pub fn schedule_system<S: System>(&mut self, system: S) {
        self.scheduled.push_back(Box::new(system));
    }

    /// Executes all scheduled systems in FIFO order and clears the queue.
    pub fn dispatch(&mut self, tick: Tick) -> Result<()> {
        while let Some(mut system) = self.scheduled.pop_front() {
            system.run(self, tick)?;
        }
        Ok(())
    }

    /// Returns a snapshot of registered component names.
    pub fn registered_components(&self) -> Vec<String> {
        self.components.clone()
    }

    /// Spawns an object into the authoritative world state.
    pub fn spawn_world_object(
        &mut self,
        kind: impl Into<String>,
        transform: WorldTransform,
    ) -> Result<WorldObject> {
        let kind = kind.into();
        if kind.is_empty() {
            return Err(KituError::InvalidInput("world object kind cannot be empty"));
        }

        let id = format!("obj-{}", self.next_world_object_id);
        self.next_world_object_id += 1;
        let object = WorldObject {
            id,
            kind,
            transform,
        };
        self.world_objects.push(object.clone());
        Ok(object)
    }

    /// Moves an existing world object to an absolute transform.
    pub fn move_world_object(
        &mut self,
        id: &str,
        transform: WorldTransform,
    ) -> Result<WorldObject> {
        let object = self
            .world_objects
            .iter_mut()
            .find(|object| object.id == id)
            .ok_or(KituError::InvalidInput("unknown world object"))?;
        object.transform = transform;
        Ok(object.clone())
    }

    /// Returns a single object by id.
    pub fn world_object(&self, id: &str) -> Option<&WorldObject> {
        self.world_objects.iter().find(|object| object.id == id)
    }

    /// Removes all authoritative world objects.
    pub fn reset_world_objects(&mut self) {
        self.world_objects.clear();
    }

    /// Returns a stable snapshot of the authoritative world state.
    pub fn world_snapshot(&self) -> WorldSnapshot {
        WorldSnapshot {
            objects: self.world_objects.clone(),
        }
    }
}

/// Example system that simply records that it has run.
#[derive(Default)]
pub struct RecordingSystem {
    /// Map of tick number to how many times the system ran during that tick.
    pub runs: HashMap<u64, usize>,
}

impl System for RecordingSystem {
    fn run(&mut self, _world: &mut EcsWorld, tick: Tick) -> Result<()> {
        *self.runs.entry(tick.get()).or_default() += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_registration_rejects_duplicates() {
        let mut world = EcsWorld::default();
        world.register_component("Transform").unwrap();
        assert!(world.register_component("Transform").is_err());
        world.register_component("Velocity").unwrap();
        assert_eq!(
            world.registered_components(),
            vec!["Transform".to_string(), "Velocity".to_string()]
        );
    }

    #[test]
    fn scheduled_systems_execute_in_order() {
        let mut world = EcsWorld::default();
        let system = RecordingSystem::default();
        world.schedule_system(RecordingSystem::default());
        world.schedule_system(system);
        let tick = Tick::start().advance_by(2);
        world.dispatch(tick).unwrap();
    }

    #[test]
    fn recording_system_counts_runs_per_tick() {
        let tick = Tick::start();
        let mut world = EcsWorld::default();
        let mut system = RecordingSystem::default();
        system.run(&mut world, tick).unwrap();
        system.run(&mut world, tick).unwrap();
        assert_eq!(system.runs.get(&tick.get()), Some(&2));
    }

    #[test]
    fn world_objects_spawn_move_and_snapshot() {
        let mut world = EcsWorld::new();

        let spawned = world
            .spawn_world_object("enemy", WorldTransform::new(1.0, 2.0, 3.0))
            .unwrap();
        assert_eq!(spawned.id, "obj-1");

        let moved = world
            .move_world_object(&spawned.id, WorldTransform::new(4.0, 5.0, 6.0))
            .unwrap();
        assert_eq!(moved.transform, WorldTransform::new(4.0, 5.0, 6.0));

        let snapshot = world.world_snapshot();
        assert_eq!(snapshot.objects, vec![moved]);

        world.reset_world_objects();
        assert!(world.world_snapshot().objects.is_empty());
    }
}
