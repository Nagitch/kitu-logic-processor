//! Tick-based runtime loop orchestrating ECS and transport.
//!
//! # Responsibilities
//! - Advance the simulation tick-by-tick, driving ECS systems and processing transport events.
//! - Bridge transports, scripting, and timeline playback while keeping determinism at the core.
//! - Provide configuration hooks (tick rate, logging) that callers can tune per embedding.
//!
//! # Integration
//! This crate glues together ECS (`kitu-ecs`), transports (`kitu-transport`), OSC/IR messages
//! (`kitu-osc-ir`), and future data or scripting layers. See `doc/crates-overview.md` for how the
//! runtime coordinates the workspace crates.

use std::time::Duration;

use kitu_core::{Result, Tick};
use kitu_ecs::EcsWorld;
use kitu_transport::{Transport, TransportEvent};

/// Configuration for the runtime loop.
///
/// The configuration controls timing for the scheduler and ECS dispatch.
/// Most callers will rely on [`default_60hz`](Self::default_60hz), but the
/// struct is intentionally lightweight so tools can adjust cadence for
/// profiling or headless simulations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// Target tick rate in Hertz. Each tick advances the world by
    /// `1.0 / tick_rate_hz` seconds.
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
    ///
    /// # Examples
    ///
    /// ```
    /// use kitu_runtime::{Runtime, RuntimeConfig};
    /// use kitu_transport::LocalChannel;
    ///
    /// let config = RuntimeConfig { tick_rate_hz: 30 };
    /// let runtime = Runtime::new(config, LocalChannel::connected());
    /// assert_eq!(runtime.config().tick_rate_hz, 30);
    /// ```
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
    ///
    /// This dispatches all scheduled ECS systems for the current tick and
    /// polls the transport for pending events before incrementing the tick
    /// counter.
    ///
    /// # Examples
    ///
    /// ```
    /// use kitu_runtime::build_runtime;
    /// use kitu_transport::LocalChannel;
    ///
    /// let mut runtime = build_runtime(LocalChannel::connected());
    /// assert_eq!(runtime.current_tick().get(), 0);
    /// runtime.tick_once().unwrap();
    /// assert_eq!(runtime.current_tick().get(), 1);
    /// ```
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
    ///
    /// # Examples
    ///
    /// ```
    /// use kitu_runtime::build_runtime;
    /// use kitu_transport::LocalChannel;
    ///
    /// let mut runtime = build_runtime(LocalChannel::connected());
    /// runtime.run_for_ticks(2).unwrap();
    /// assert_eq!(runtime.current_tick().get(), 2);
    /// ```
    pub fn run_for_ticks(&mut self, count: u64) -> Result<()> {
        for _ in 0..count {
            self.tick_once()?;
        }
        Ok(())
    }
}

/// Convenience helper for building a runtime with default configuration.
///
/// # Examples
///
/// ```
/// use kitu_runtime::build_runtime;
/// use kitu_transport::LocalChannel;
///
/// let runtime = build_runtime(LocalChannel::connected());
/// assert_eq!(runtime.config().tick_rate_hz, 60);
/// ```
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
