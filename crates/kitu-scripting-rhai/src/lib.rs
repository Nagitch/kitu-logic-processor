//! Bounded Rhai function calls with copied JSON inputs.
//!
//! The host has no application, filesystem, clock, logging or network bindings.
//! Applications validate returned requests before applying game changes and pin
//! the source, [`RHAI_VERSION`] and [`POLICY_VERSION`] in replay metadata.
//!
//! ```
//! use kitu_scripting_rhai::{Limits, ScriptHost};
//! use serde_json::json;
//! let host = ScriptHost::new(Limits::default())?;
//! let program = host.compile(r#"
//!     fn boss(input) {
//!         if input.timerExpired { #{action: "telegraph", duration: 0.8} }
//!         else { #{action: "wait", duration: 0.0} }
//!     }
//! "#)?;
//! assert_eq!(host.invoke(&program, "boss", &json!({"timerExpired": true}))?,
//!            json!({"action": "telegraph", "duration": 0.8}));
//! # Ok::<(), kitu_scripting_rhai::Diagnostic>(())
//! ```

mod data;
mod diagnostic;

pub use diagnostic::Diagnostic;
use rhai::{
    packages::{ArithmeticPackage, LogicPackage, Package},
    Engine, OptimizationLevel, Scope, AST,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Execution policy identifier; capability or default-limit changes require a new version.
pub const POLICY_VERSION: &str = "kitu-rhai-json-v1";
/// Exact engine version pinned by this crate's dependency contract.
pub const RHAI_VERSION: &str = "1.26.0";

/// Deterministic bounds for compilation, evaluation and copied JSON data.
///
/// All bounds must be nonzero and within the ceilings checked by
/// [`ScriptHost::new`]. Programs can be invoked by another host only when their
/// limits are identical. Zero never means unlimited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    /// Maximum source UTF-8 bytes; default 65,536, ceiling 262,144.
    pub max_source_bytes: usize,
    /// Maximum serialized input JSON bytes; default 16,384, ceiling 262,144.
    pub max_input_bytes: usize,
    /// Maximum serialized output JSON bytes; default 16,384, ceiling 262,144.
    pub max_output_bytes: usize,
    /// Operations per call including top-level code; default 10,000, ceiling 1,000,000.
    pub max_operations: u64,
    /// Script call depth; default 32, ceiling 64.
    pub max_call_levels: usize,
    /// Global and function expression nesting; default 32, ceiling 64.
    pub max_expression_depth: usize,
    /// Functions in one compilation; default 64, ceiling 256.
    pub max_functions: usize,
    /// Variables in one scope; default 128, ceiling 256.
    pub max_variables: usize,
    /// String/key bytes; Rhai also aggregates strings in a container. Default 4,096, ceiling 65,536.
    pub max_string_bytes: usize,
    /// Array entries; Rhai also counts nested array entries. Default 256, ceiling 4,096.
    pub max_array_len: usize,
    /// Object entries; Rhai also counts nested object entries. Default 128, ceiling 1,024.
    pub max_map_len: usize,
    /// JSON nesting depth, root at zero; default 16, ceiling 32.
    pub max_json_depth: usize,
    /// Values in one input/output including containers; default 4,096, ceiling 16,384.
    pub max_json_nodes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_source_bytes: 65_536,
            max_input_bytes: 16_384,
            max_output_bytes: 16_384,
            max_operations: 10_000,
            max_call_levels: 32,
            max_expression_depth: 32,
            max_functions: 64,
            max_variables: 128,
            max_string_bytes: 4_096,
            max_array_len: 256,
            max_map_len: 128,
            max_json_depth: 16,
            max_json_nodes: 4_096,
        }
    }
}

impl Limits {
    fn validate(&self) -> Result<(), Diagnostic> {
        for (name, value, ceiling) in [
            ("source bytes", self.max_source_bytes, 262_144),
            ("input bytes", self.max_input_bytes, 262_144),
            ("output bytes", self.max_output_bytes, 262_144),
            ("call levels", self.max_call_levels, 64),
            ("expression depth", self.max_expression_depth, 64),
            ("functions", self.max_functions, 256),
            ("variables", self.max_variables, 256),
            ("string bytes", self.max_string_bytes, 65_536),
            ("array length", self.max_array_len, 4_096),
            ("map length", self.max_map_len, 1_024),
            ("JSON depth", self.max_json_depth, 32),
            ("JSON nodes", self.max_json_nodes, 16_384),
        ] {
            if value == 0 || value > ceiling {
                return Err(Diagnostic::new(
                    "invalidLimits",
                    format_args!("{name} must be in 1..={ceiling}"),
                ));
            }
        }
        if self.max_operations == 0 || self.max_operations > 1_000_000 {
            return Err(Diagnostic::new(
                "invalidLimits",
                "operations must be in 1..=1000000",
            ));
        }
        Ok(())
    }
}

/// Cloneable compiled code without mutable application or per-call state.
///
/// The opaque AST prevents callers from adding native functions or modules.
#[derive(Debug, Clone)]
pub struct CompiledScript {
    ast: AST,
    limits: Limits,
}

/// A shareable engine exposing arithmetic, comparisons and core syntax.
///
/// No standard package, host callbacks, modules or external state are exposed.
/// Compilation performs no evaluation; every invocation starts with a fresh scope.
pub struct ScriptHost {
    engine: Engine,
    limits: Limits,
}

impl std::fmt::Debug for ScriptHost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScriptHost")
            .field("policy", &POLICY_VERSION)
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl ScriptHost {
    /// Construct a restricted engine, rejecting zero or excessive bounds.
    pub fn new(limits: Limits) -> Result<Self, Diagnostic> {
        limits.validate()?;
        let mut engine = Engine::new_raw();
        ArithmeticPackage::new().register_into_engine(&mut engine);
        LogicPackage::new().register_into_engine(&mut engine);
        // Engine::new_raw() has no module resolver. Do not call module-specific
        // APIs: Tanu's existing no_module feature removes them in combined builds.
        engine.set_optimization_level(OptimizationLevel::None);
        engine.set_allow_anonymous_fn(false);
        for symbol in [
            "eval", "import", "export", "print", "debug", "Fn", "call", "curry",
        ] {
            engine.disable_symbol(symbol);
        }
        engine.set_max_operations(limits.max_operations);
        engine.set_max_call_levels(limits.max_call_levels);
        engine.set_max_expr_depths(limits.max_expression_depth, limits.max_expression_depth);
        engine.set_max_functions(limits.max_functions);
        engine.set_max_variables(limits.max_variables);
        engine.set_max_string_size(limits.max_string_bytes);
        engine.set_max_array_size(limits.max_array_len);
        engine.set_max_map_size(limits.max_map_len);
        Ok(Self { engine, limits })
    }

    /// Compile bounded UTF-8 source without executing top-level code.
    ///
    /// This validates syntax, not every execution branch. Applications must probe
    /// representative contexts and reject invalid requests before changing state.
    pub fn compile(&self, source: &str) -> Result<CompiledScript, Diagnostic> {
        if source.len() > self.limits.max_source_bytes {
            return Err(Diagnostic::new("sourceLimit", "source exceeds byte limit"));
        }
        let ast = self
            .engine
            .compile(source)
            .map_err(|error| Diagnostic::positioned("compile", &error, error.position()))?;
        Ok(CompiledScript {
            ast,
            limits: self.limits.clone(),
        })
    }

    /// Call one script function with one copied JSON argument and return JSON.
    ///
    /// Top-level code runs afresh inside the same operation budget. Script
    /// mutations never change the caller's JSON. Output accepts null, booleans,
    /// signed 64-bit integers, finite binary64 numbers, strings, arrays and objects.
    /// Invalid data, mismatched limits, missing entrypoints and execution failures
    /// produce bounded structured diagnostics.
    pub fn invoke(
        &self,
        program: &CompiledScript,
        entrypoint: &str,
        input: &Value,
    ) -> Result<Value, Diagnostic> {
        if program.limits != self.limits {
            return Err(Diagnostic::new(
                "policyMismatch",
                "compiled script and host limits differ",
            ));
        }
        if entrypoint.is_empty()
            || entrypoint.len() > 128
            || !entrypoint.bytes().enumerate().all(|(i, c)| {
                c.is_ascii_alphabetic() || c == b'_' || (i != 0 && c.is_ascii_digit())
            })
        {
            return Err(Diagnostic::new(
                "entrypoint",
                "entrypoint must be a bounded ASCII identifier",
            ));
        }
        data::validate_input(input, &self.limits)?;
        let input =
            rhai::serde::to_dynamic(input).map_err(|error| Diagnostic::new("input", error))?;
        let output = self
            .engine
            .call_fn::<rhai::Dynamic>(&mut Scope::new(), &program.ast, entrypoint, (input,))
            .map_err(|error| Diagnostic::evaluation(&error))?;
        data::output_json(&output, &self.limits)
    }
}

#[cfg(test)]
mod tests;
