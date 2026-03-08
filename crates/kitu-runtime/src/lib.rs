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

use std::{collections::VecDeque, time::Duration};

use kitu_core::{KituError, Result, Tick};
use kitu_ecs::EcsWorld;
use kitu_osc_ir::OscBundle;
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
    accumulator: Duration,
    transport: T,
    world: EcsWorld,
    committed_inputs: VecDeque<OscBundle>,
    pending_inputs: VecDeque<OscBundle>,
    staged_outputs: VecDeque<OscBundle>,
    output_buffer: VecDeque<OscBundle>,
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
            accumulator: Duration::ZERO,
            transport,
            world: EcsWorld::default(),
            committed_inputs: VecDeque::new(),
            pending_inputs: VecDeque::new(),
            staged_outputs: VecDeque::new(),
            output_buffer: VecDeque::new(),
        }
    }

    /// Returns the world instance for registering systems and components.
    pub fn world_mut(&mut self) -> &mut EcsWorld {
        &mut self.world
    }

    /// Enqueues an input bundle for the next tick.
    pub fn enqueue_input(&mut self, input: OscBundle) {
        self.pending_inputs.push_back(input);
    }

    /// Stages an output bundle that becomes visible after the current tick.
    pub fn queue_output(&mut self, output: OscBundle) {
        self.staged_outputs.push_back(output);
    }

    /// Drains all emitted output bundles in FIFO order.
    pub fn drain_output_buffer(&mut self) -> Vec<OscBundle> {
        self.output_buffer.drain(..).collect()
    }

    /// Drains committed input bundles in FIFO order.
    ///
    /// Input bundles are committed at the beginning of a tick and remain
    /// available until explicitly drained by the caller.
    pub fn drain_committed_inputs(&mut self) -> Vec<OscBundle> {
        self.committed_inputs.drain(..).collect()
    }

    /// Processes as many fixed ticks as `dt` allows.
    ///
    /// Returns how many ticks were executed.
    pub fn update(&mut self, dt: f32) -> Result<u32> {
        if !dt.is_finite() {
            return Err(KituError::InvalidInput("dt must be finite"));
        }

        if dt.is_sign_negative() {
            return Err(KituError::InvalidInput("dt must be non-negative"));
        }

        let dt_secs = f64::from(dt);
        if dt_secs > Duration::MAX.as_secs_f64() {
            return Err(KituError::InvalidInput("dt is too large"));
        }

        if self.config.tick_rate_hz == 0 {
            return Err(KituError::InvalidInput(
                "tick_rate_hz must be greater than zero",
            ));
        }

        self.accumulator += Duration::from_secs_f64(dt_secs);
        let frame_time = self.config.frame_time();
        if frame_time.is_zero() {
            return Err(KituError::InvalidInput(
                "frame_time must be greater than zero",
            ));
        }

        let mut executed = 0;

        while self.accumulator >= frame_time {
            self.tick_once()?;
            self.accumulator -= frame_time;
            executed += 1;
        }

        Ok(executed)
    }

    /// Processes a single tick of the runtime loop.
    ///
    /// This dispatches all scheduled ECS systems for the current tick, emits
    /// staged outputs, polls transport events, and increments the tick counter.
    /// Inputs received while polling are queued for the next tick.
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
        self.committed_inputs.append(&mut self.pending_inputs);

        self.world.dispatch(self.tick)?;

        self.output_buffer.append(&mut self.staged_outputs);

        while let Some(event) = self.transport.poll_event() {
            if let TransportEvent::Message(bundle) = event {
                self.pending_inputs.push_back(bundle);
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
    fn update_uses_fixed_timestep_accumulator() {
        let mut runtime = build_runtime(LocalChannel::default());

        let executed = runtime.update(0.010).unwrap();
        assert_eq!(executed, 0);
        assert_eq!(runtime.current_tick().get(), 0);

        let executed = runtime.update(0.010).unwrap();
        assert_eq!(executed, 1);
        assert_eq!(runtime.current_tick().get(), 1);
    }

    #[test]
    fn update_rejects_non_finite_dt() {
        let mut runtime = build_runtime(LocalChannel::default());
        assert!(runtime.update(f32::NAN).is_err());
        assert!(runtime.update(f32::INFINITY).is_err());
    }

    #[test]
    fn update_rejects_oversized_dt() {
        let mut runtime = build_runtime(LocalChannel::default());
        assert!(runtime.update(f32::MAX).is_err());
    }

    #[test]
    fn update_rejects_invalid_tick_rate() {
        let mut runtime = Runtime::new(RuntimeConfig { tick_rate_hz: 0 }, LocalChannel::default());
        assert!(runtime.update(0.016).is_err());

        let mut runtime = Runtime::new(
            RuntimeConfig {
                tick_rate_hz: u32::MAX,
            },
            LocalChannel::default(),
        );
        assert!(runtime.update(0.016).is_err());
    }

    #[test]
    fn tick_applies_transport_input_on_next_tick() {
        struct ScriptedTransport {
            events: VecDeque<TransportEvent>,
        }

        impl Transport for ScriptedTransport {
            fn send(&mut self, _message: kitu_osc_ir::OscMessage) -> Result<()> {
                Ok(())
            }

            fn poll_event(&mut self) -> Option<TransportEvent> {
                self.events.pop_front()
            }
        }

        let mut bundle = OscBundle::new();
        bundle.push(kitu_osc_ir::OscMessage::new("/input/move"));

        let transport = ScriptedTransport {
            events: VecDeque::from([TransportEvent::Message(bundle)]),
        };

        let mut runtime = build_runtime(transport);

        runtime.tick_once().unwrap();
        assert_eq!(runtime.committed_inputs.len(), 0);
        assert_eq!(runtime.pending_inputs.len(), 1);

        runtime.tick_once().unwrap();
        let committed = runtime.drain_committed_inputs();
        assert_eq!(committed.len(), 1);
        assert_eq!(runtime.pending_inputs.len(), 0);
    }

    #[test]
    fn committed_inputs_are_preserved_until_drained() {
        let mut runtime = build_runtime(LocalChannel::default());
        let mut input = OscBundle::new();
        input.push(kitu_osc_ir::OscMessage::new("/input/attack"));
        runtime.enqueue_input(input.clone());

        runtime.tick_once().unwrap();
        runtime.tick_once().unwrap();

        let committed = runtime.drain_committed_inputs();
        assert_eq!(committed, vec![input]);
    }

    #[test]
    fn outputs_are_emitted_after_tick() {
        let mut runtime = build_runtime(LocalChannel::default());
        let mut output = OscBundle::new();
        output.push(kitu_osc_ir::OscMessage::new("/render/player/transform"));

        runtime.queue_output(output.clone());
        assert!(runtime.drain_output_buffer().is_empty());

        runtime.tick_once().unwrap();
        let drained = runtime.drain_output_buffer();
        assert_eq!(drained, vec![output]);
    }
}
