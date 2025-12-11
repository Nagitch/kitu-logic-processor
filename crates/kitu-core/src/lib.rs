//! Core types and utilities shared across the Kitu runtime crates.
//!
//! # Responsibilities
//! - Provide the shared [`KituError`] type and [`Result`] alias used across workspace crates.
//! - Define tick and timestamp helpers that keep scheduling consistent between runtime and tools.
//! - Host small, dependency-light utilities that other crates can import without pulling heavy stacks.
//!
//! # Integration
//! This crate underpins the ECS (`kitu-ecs`), transport (`kitu-transport`), timeline (`kitu-tsq1`),
//! and frontend bindings by centralizing foundational primitives. See `doc/crates-overview.md` for a
//! workspace map and module responsibilities.

use std::time::Duration;

use thiserror::Error;

/// Convenient result alias used across Kitu crates.
pub type Result<T, E = KituError> = std::result::Result<T, E>;

/// Unified error enum for the core crates. Additional variants should be added as
/// behavior becomes concrete.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum KituError {
    /// Placeholder for unimplemented or unconfigured features.
    #[error("not implemented: {0}")]
    NotImplemented(String),

    /// Represents invalid user or data input.
    #[error("invalid input: {0}")]
    InvalidInput(&'static str),
}

/// Tick represents a deterministic, monotonic counter for the runtime loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tick(u64);

impl Tick {
    /// Creates a new tick starting at zero.
    pub const fn start() -> Self {
        Self(0)
    }

    /// Advances the tick by one.
    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }

    /// Advances the tick by the provided offset.
    pub const fn advance_by(self, offset: u64) -> Self {
        Self(self.0 + offset)
    }

    /// Returns the raw tick counter value.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Represents a monotonically increasing timestamp based on tick duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Timestamp {
    tick: Tick,
    frame_time: Duration,
}

impl Timestamp {
    /// Creates a timestamp from the given tick and per-frame duration.
    pub const fn new(tick: Tick, frame_time: Duration) -> Self {
        Self { tick, frame_time }
    }

    /// Returns elapsed time since tick zero.
    pub fn elapsed(&self) -> Duration {
        self.frame_time * self.tick.0 as u32
    }

    /// Accessor for the internal tick.
    pub const fn tick(&self) -> Tick {
        self.tick
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_advances_monotonically() {
        let start = Tick::start();
        let next = start.next();
        assert_eq!(start.get(), 0);
        assert_eq!(next.get(), 1);
        assert!(next > start);
    }

    #[test]
    fn timestamp_reports_elapsed_duration() {
        let frame_time = Duration::from_millis(16);
        let ts = Timestamp::new(Tick::start().advance_by(3), frame_time);
        assert_eq!(ts.elapsed(), frame_time * 3);
        assert_eq!(ts.tick(), Tick::start().advance_by(3));
    }

    #[test]
    fn errors_display_meaningful_messages() {
        let not_impl = KituError::NotImplemented("feature".into()).to_string();
        let invalid = KituError::InvalidInput("bad").to_string();
        assert!(not_impl.contains("feature"));
        assert!(invalid.contains("bad"));
    }
}
