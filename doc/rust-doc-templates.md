# Rust Documentation Templates

This document provides copy-paste ready templates for crate-level, module-level, and public API documentation
to be used across the `kitu-logic-processor` repository.

The goal is to keep documentation consistent, clear, and aligned with Rust best practices.


## Crate and Module Documentation

### Crate-level (`src/lib.rs`)

```rust
//! Kitu Logic Processor core crate.
//!
//! This crate provides deterministic, testable logic execution for
//! Kitu-based applications, independent of any specific frontend.
//!
//! # Features
//! - ECS-based world representation
//! - TSQ1-style timeline processing
//! - Data-driven configuration (e.g. Tanu Markdown, DB-backed configs)
//! - Integration points for Unity and other runtimes
//!
//! # Examples
//! Basic usage patterns and higher-level workflows should be documented
//! in each public module and in the `doc/` directory.
```

### Module-level (`src/ecs/mod.rs`, etc.)

```rust
//! ECS core module.
//!
//! Provides world, entities, and systems used by the Kitu Logic Processor.
//!
//! # Responsibilities
//! - Maintain entity and component storage
//! - Run systems in a deterministic order
//! - Expose a stable API for scheduling and ticking the world
```


## Public API Documentation Templates

### Struct

```rust
/// Represents a logical time in the Kitu timeline.
///
/// This is a thin wrapper over milliseconds that provides utility
/// methods for common conversions and comparisons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeMs(pub u64);
```

### Enum

```rust
/// Describes the current playback state of a timeline.
///
/// This state is used by the scheduler and frontends to reason about
/// user interactions (play / pause / stop / scrubbing).
pub enum PlaybackState {
    /// Timeline is currently stopped and positioned at a fixed time.
    Stopped,
    /// Timeline is currently playing forward in real time.
    Playing,
    /// Timeline is currently paused, but can be resumed.
    Paused,
}
```

### Trait

```rust
/// A system that updates part of the world each tick.
///
/// Systems are expected to be deterministic and free of side effects
/// outside the provided `World` and `Context`.
pub trait System {
    /// Runs the system for a single tick.
    ///
    /// # Parameters
    /// - `world`: Mutable world state to operate on.
    /// - `ctx`:   Tick context (time, configuration, etc.).
    ///
    /// # Errors
    /// Implementations may return an error if the system cannot complete
    /// its work. Callers should treat this as a fatal error for the tick.
    fn run(&mut self, world: &mut World, ctx: &TickContext) -> Result<(), SystemError>;
}
```

### Function (with errors / panics)

```rust
/// Advances the world by a single tick.
///
/// This function updates the world state and processes all scheduled
/// events up to the given `delta` from the current time.
///
/// # Parameters
/// - `delta`: Duration to advance the simulation by.
///
/// # Examples
///
/// ```
/// # use kitu_logic::world::World;
/// # use std::time::Duration;
/// let mut world = World::new();
/// world.tick(Duration::from_millis(16)).unwrap();
/// ```
///
/// # Errors
///
/// Returns an error if any system fails during execution, or if the
/// scheduler encounters an unrecoverable inconsistency.
///
/// # Panics
///
/// This function must not panic under normal usage. If it does, it is
/// considered a bug and should be fixed.
pub fn tick(&mut self, delta: Duration) -> Result<(), TickError> {
    // ...
}
```

### Error Type (`thiserror`)

```rust
/// Errors that can occur while ticking the world.
#[derive(Debug, thiserror::Error)]
pub enum TickError {
    /// An internal system failed during execution.
    #[error("system `{name}` failed: {source}")]
    SystemFailure {
        name: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// The scheduler detected an inconsistent or invalid state.
    #[error("scheduler inconsistency: {details}")]
    SchedulerInconsistency { details: String },
}
```


## Doctest-Oriented Examples

Use the following pattern for doctest-friendly examples in `# Examples` sections:

```rust
/// Does something useful in the Kitu world.
///
/// # Examples
///
/// ```
/// # use kitu_logic::world::World;
/// let mut world = World::new();
/// world.do_something().unwrap();
/// ```
pub fn do_something(&mut self) -> Result<(), DoSomethingError> {
    // ...
}
```

Notes:

- Lines starting with `#` inside the code block are executed in doctests
  but hidden from the rendered documentation. Use them for setup code.
- Prefer minimal, focused examples that still compile and run.
