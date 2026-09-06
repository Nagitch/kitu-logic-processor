//! Persistent application hooks executed inside the runtime input/output barriers.

use kitu_core::{Result, Tick};
use kitu_ecs::EcsWorld;
use kitu_osc_ir::OscBundle;

/// Identity and schema information retained alongside a logical input bundle.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputMetadata {
    /// Stable producer identity, retained across reconnects to the same runtime.
    pub source: String,
    /// Producer-scoped monotonically allocated operation identifier.
    pub message_id: u64,
    /// Application contract version, independent of wire encoding.
    pub schema_version: u32,
}

/// A logical bundle with optional application envelope metadata.
#[derive(Debug, Clone)]
pub struct RuntimeInput {
    /// Runtime-assigned enqueue order, independent of producer IDs and wire timing.
    pub sequence: u64,
    /// Original ordered logical messages.
    pub bundle: OscBundle,
    /// Application identity; legacy movement and transport inputs may omit it.
    pub metadata: Option<InputMetadata>,
}

/// Immutable committed input context for a single authoritative tick.
pub struct ApplicationTick<'a> {
    /// Authoritative source tick, before the runtime increments it.
    pub tick: Tick,
    /// Configured fixed timestep in seconds.
    pub dt: f32,
    /// Frozen input batch; inputs arriving during this tick belong to a later tick.
    pub inputs: &'a [RuntimeInput],
}

/// Application behavior installed before the runtime's first tick.
///
/// Keep state in typed [`EcsWorld`] resources and use `snapshot` for detached
/// projections. Validate fallible input parsing before any system dispatch.
/// After validation, `tick` commits an infallible rule update: state-dependent
/// rejections are ordinary outputs, not runtime failures that invite retries.
/// Implementations must not read wall-clock time or perform host I/O in a tick.
pub trait RuntimeApplication: Send + Sync + 'static {
    /// Validates structural input requirements without mutating application state.
    fn validate_inputs(&self, inputs: &[RuntimeInput]) -> Result<()>;

    /// Updates world-owned state and returns outputs staged for the current barrier.
    fn tick(&mut self, world: &mut EcsWorld, context: ApplicationTick<'_>) -> Vec<OscBundle>;

    /// Returns detached projection messages without advancing or mutating the world.
    fn snapshot(&self, world: &EcsWorld) -> Vec<OscBundle>;
}
