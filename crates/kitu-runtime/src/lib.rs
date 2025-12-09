//! Tick-based runtime loop orchestrating ECS and transport.

use std::time::Duration;

use kitu_core::{Result, Tick};
use kitu_ecs::EcsWorld;
use kitu_transport::{Transport, TransportEvent};

/// Configuration for the runtime loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub tick_rate_hz: u32,
}

impl RuntimeConfig {
    /// Creates a configuration with a default 60Hz tick rate.
    pub const fn default_60hz() -> Self {
        Self { tick_rate_hz: 60 }
    }

    /// Duration for a single frame.
    pub fn frame_time(&self) -> Duration {
        Duration::from_secs_f32(1.0 / self.tick_rate_hz as f32)
    }
}

/// Central orchestrator tying together ECS and message transport.
pub struct Runtime<T: Transport> {
    config: RuntimeConfig,
    tick: Tick,
    transport: T,
    world: EcsWorld,
}

impl<T: Transport> Runtime<T> {
    /// Creates a new runtime with the given transport and configuration.
    pub fn new(config: RuntimeConfig, transport: T) -> Self {
        Self {
            config,
            tick: Tick::start(),
            transport,
            world: EcsWorld::default(),
        }
    }

    /// Returns the world instance for registering systems and components.
    pub fn world_mut(&mut self) -> &mut EcsWorld {
        &mut self.world
    }

    /// Processes a single tick of the runtime loop.
    pub fn tick_once(&mut self) -> Result<()> {
        self.world.dispatch(self.tick)?;
        while let Some(event) = self.transport.poll_event() {
            if let TransportEvent::Message(_) = event {
                // Future work: route to ECS systems or script hooks.
            }
        }
        self.tick = self.tick.next();
        Ok(())
    }

    /// Returns the current tick counter.
    pub fn current_tick(&self) -> Tick {
        self.tick
    }

    /// Returns the configuration used to construct the runtime.
    pub fn config(&self) -> RuntimeConfig {
        self.config
    }

    /// Runs the runtime for the requested number of ticks.
    pub fn run_for_ticks(&mut self, count: u64) -> Result<()> {
        for _ in 0..count {
            self.tick_once()?;
        }
        Ok(())
    }
}

/// Convenience helper for building a runtime with default configuration.
pub fn build_runtime<T: Transport>(transport: T) -> Runtime<T> {
    Runtime::new(RuntimeConfig::default_60hz(), transport)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kitu_core::KituError;
    use kitu_transport::LocalChannel;

    #[test]
    fn runtime_advances_ticks() {
        let mut runtime = build_runtime(LocalChannel::default());
        assert_eq!(runtime.current_tick().get(), 0);
        runtime.tick_once().unwrap();
        assert_eq!(runtime.current_tick().get(), 1);
    }

    #[test]
    fn runtime_processes_multiple_ticks() {
        let mut runtime = build_runtime(LocalChannel::default());
        runtime.run_for_ticks(3).unwrap();
        assert_eq!(runtime.current_tick().get(), 3);
    }

    #[test]
    fn frame_time_matches_tick_rate() {
        let config = RuntimeConfig { tick_rate_hz: 120 };
        let frame = config.frame_time();
        let expected = 1.0 / 120.0;
        let actual = frame.as_secs_f64();
        assert!((actual - expected).abs() < 1e-6);
    }

    #[test]
    fn tick_once_returns_error_when_dispatch_fails() {
        struct FailingTransport;
        impl Transport for FailingTransport {
            fn send(&mut self, _message: kitu_osc_ir::OscMessage) -> Result<()> {
                Err(KituError::NotImplemented("send".into()))
            }

            fn poll_event(&mut self) -> Option<TransportEvent> {
                None
            }
        }

        let mut runtime = build_runtime(FailingTransport);
        assert!(runtime.tick_once().is_ok());
    }
}
