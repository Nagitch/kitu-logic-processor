use rhai::{EvalAltResult, Position};
use serde::{Deserialize, Serialize};
use std::fmt::{self, Write};

/// Bounded engine diagnostics for CLI, Admin and replay validation.
///
/// Generated messages contain at most 1,024 UTF-8 bytes. Positions are one-based
/// when supplied by Rhai; host/data errors have no source position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Diagnostic {
    /// Stable category, such as `compile`, `runtime`, `executionLimit` or `inputLimit`.
    pub kind: String,
    /// Human-readable bounded diagnostic.
    pub message: String,
    /// One-based source line if known.
    pub line: Option<usize>,
    /// One-based source column if known.
    pub column: Option<usize>,
}

struct Message(String);
impl Write for Message {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let remaining = 1024 - self.0.len();
        let mut end = value.len().min(remaining);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.0.push_str(&value[..end]);
        if end != value.len() {
            Err(fmt::Error)
        } else {
            Ok(())
        }
    }
}

impl Diagnostic {
    pub(crate) fn new(kind: &str, message: impl fmt::Display) -> Self {
        Self::positioned(kind, message, Position::NONE)
    }

    pub(crate) fn positioned(kind: &str, message: impl fmt::Display, position: Position) -> Self {
        let mut output = Message(String::new());
        let _ = write!(output, "{message}");
        Self {
            kind: kind.into(),
            message: output.0,
            line: position.line(),
            column: position.position(),
        }
    }

    pub(crate) fn evaluation(error: &EvalAltResult) -> Self {
        let mut cause = error;
        while let EvalAltResult::ErrorInFunctionCall(_, _, inner, _) = cause {
            cause = inner;
        }
        let kind = match cause {
            EvalAltResult::ErrorTooManyOperations(_)
            | EvalAltResult::ErrorTooManyVariables(_)
            | EvalAltResult::ErrorTooManyModules(_)
            | EvalAltResult::ErrorStackOverflow(_)
            | EvalAltResult::ErrorDataTooLarge(_, _) => "executionLimit",
            _ => "runtime",
        };
        Self::positioned(kind, error, cause.position())
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.message)?;
        if let Some(line) = self.line {
            write!(formatter, " (line {line}")?;
            if let Some(column) = self.column {
                write!(formatter, ", column {column}")?;
            }
            write!(formatter, ")")?;
        }
        Ok(())
    }
}
impl std::error::Error for Diagnostic {}
