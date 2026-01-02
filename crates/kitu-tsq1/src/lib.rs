//! TSQ1 timeline parser and scheduler skeleton.
//!
//! # Responsibilities
//! - Parse TSQ1 text into a structured AST for deterministic playback.
//! - Provide scheduling helpers that emit OSC/IR events for the runtime loop.
//! - Keep the timeline model self-contained so tooling and runtime share the same semantics.
//!
//! # Integration
//! Timeline playback produced here can be consumed by transports (`kitu-transport`) and the runtime
//! (`kitu-runtime`). See `doc/crates-overview.md` for how TSQ1 fits into the execution pipeline.

use std::collections::VecDeque;

use kitu_core::{KituError, Result, Tick};

/// A single timeline step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineStep {
    /// Emits the provided marker or event label.
    Emit(String),
    /// Pauses execution for the specified number of ticks before continuing.
    Wait(u64),
}

/// Parsed TSQ1 script consisting of ordered steps.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Timeline {
    steps: VecDeque<TimelineStep>,
}

impl Timeline {
    /// Parses a very small subset of TSQ1 where each line is either `emit:value` or `wait:n`.
    ///
    /// # Examples
    ///
    /// ```
    /// use kitu_tsq1::{Timeline, TimelineStep};
    ///
    /// let script = "emit:start\nwait:1\nemit:end";
    /// let timeline = Timeline::parse(script).unwrap();
    /// assert_eq!(timeline.len(), 3);
    /// assert!(!timeline.is_finished());
    /// ```
    pub fn parse(script: &str) -> Result<Self> {
        let mut steps = VecDeque::new();
        for line in script.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix("emit:") {
                steps.push_back(TimelineStep::Emit(rest.trim().to_string()));
            } else if let Some(rest) = trimmed.strip_prefix("wait:") {
                let ticks: u64 = rest
                    .trim()
                    .parse()
                    .map_err(|_| KituError::InvalidInput("wait expects integer"))?;
                steps.push_back(TimelineStep::Wait(ticks));
            } else {
                return Err(KituError::InvalidInput("unknown directive"));
            }
        }
        Ok(Self { steps })
    }

    /// Pops the next step for the provided tick.
    pub fn next_step(&mut self, _tick: Tick) -> Option<TimelineStep> {
        self.steps.pop_front()
    }

    /// Returns whether all steps have been consumed.
    pub fn is_finished(&self) -> bool {
        self.steps.is_empty()
    }

    /// Returns the number of queued steps remaining.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Returns `true` if no steps are queued.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_emit_and_wait_steps() {
        let script = "emit: start\nwait:2\nemit: end";
        let mut timeline = Timeline::parse(script).unwrap();
        assert_eq!(
            timeline.next_step(Tick::start()),
            Some(TimelineStep::Emit("start".into()))
        );
        assert_eq!(
            timeline.next_step(Tick::start()),
            Some(TimelineStep::Wait(2))
        );
        assert_eq!(
            timeline.next_step(Tick::start()),
            Some(TimelineStep::Emit("end".into()))
        );
        assert!(timeline.is_finished());
    }

    #[test]
    fn len_reports_remaining_steps() {
        let script = "emit: start\nemit: end";
        let timeline = Timeline::parse(script).unwrap();
        assert_eq!(timeline.len(), 2);
    }

    #[test]
    fn unknown_directives_error() {
        let script = "noop";
        assert!(Timeline::parse(script).is_err());
    }
}
