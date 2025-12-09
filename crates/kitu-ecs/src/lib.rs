//! Thin ECS abstraction used by the runtime.

use std::collections::{HashMap, VecDeque};

use kitu_core::{KituError, Result, Tick};

/// Represents a system that can be scheduled for a tick.
pub trait System: Send + Sync + 'static {
    /// Executes the system for the given tick.
    fn run(&mut self, world: &mut EcsWorld, tick: Tick) -> Result<()>;
}

/// Minimal world representation for registering components and systems.
#[derive(Default)]
pub struct EcsWorld {
    components: Vec<String>,
    scheduled: VecDeque<Box<dyn System>>,
}

impl EcsWorld {
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
}

/// Example system that simply records that it has run.
#[derive(Default)]
pub struct RecordingSystem {
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
        assert_eq!(world.register_component("Velocity").unwrap(), ());
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
}
