//! TSQ1 timeline parser and scheduler skeleton.

use std::collections::VecDeque;

use kitu_core::{KituError, Result, Tick};

/// A single timeline step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineStep {
    Emit(String),
    Wait(u64),
}

/// Parsed TSQ1 script consisting of ordered steps.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Timeline {
    steps: VecDeque<TimelineStep>,
}

impl Timeline {
    /// Parses a very small subset of TSQ1 where each line is either `emit:value` or `wait:n`.
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
    fn unknown_directives_error() {
        let script = "noop";
        assert!(Timeline::parse(script).is_err());
    }
}
