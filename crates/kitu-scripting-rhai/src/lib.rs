//! Rhai scripting integration skeleton.

use std::collections::HashMap;

use kitu_core::{KituError, Result};

/// Represents a compiled or cached Rhai script (placeholder for now).
#[derive(Debug, Clone)]
pub struct Script {
    /// Symbolic script name used for lookups.
    pub name: String,
    /// Original source text associated with the script.
    pub source: String,
}

/// Hosts registered scripts and provides an entry point to invoke them.
#[derive(Default)]
pub struct ScriptHost {
    scripts: HashMap<String, Script>,
}

impl ScriptHost {
    /// Registers a script under a given name.
    pub fn register_script(&mut self, name: impl Into<String>, source: impl Into<String>) {
        let script = Script {
            name: name.into(),
            source: source.into(),
        };
        self.scripts.insert(script.name.clone(), script);
    }

    /// Invokes a function within the named script.
    pub fn invoke(&self, script: &str, func: &str) -> Result<()> {
        if !self.scripts.contains_key(script) {
            return Err(KituError::InvalidInput("missing script"));
        }
        // Actual Rhai evaluation will be added later.
        Err(KituError::NotImplemented(func.to_string()))
    }

    /// Returns the number of registered scripts.
    pub fn len(&self) -> usize {
        self.scripts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registering_and_invoking_scripts() {
        let mut host = ScriptHost::default();
        host.register_script("demo", "fn run() { 1 }");
        assert_eq!(host.len(), 1);

        let err = host.invoke("demo", "run").unwrap_err();
        assert!(matches!(err, KituError::NotImplemented(value) if value == "run"));
    }

    #[test]
    fn invoking_missing_script_returns_error() {
        let host = ScriptHost::default();
        let err = host.invoke("missing", "run").unwrap_err();
        assert!(matches!(err, KituError::InvalidInput("missing script")));
    }
}
