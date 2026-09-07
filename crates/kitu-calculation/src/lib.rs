//! Kitu adapter for the shared typed calculation kernel.

pub use openformula_kernel::{
    Argument, CalcError, CalcErrorKind, CalcResult, CoercionPolicy, EvalContext, FunctionMetadata,
    Number, PureContext, Value,
};

use openformula_kernel::extensions::register_kitu_examples;
use openformula_kernel::FunctionRegistry;

/// Kitu's registry boundary for standard and namespaced calculation functions.
#[derive(Clone)]
pub struct KituCalculationKernel {
    registry: FunctionRegistry,
}

impl KituCalculationKernel {
    /// Construct a registry containing only the shared standard subset.
    #[must_use]
    pub fn standard() -> Self {
        Self {
            registry: FunctionRegistry::standard(),
        }
    }

    /// Explicitly install the versioned `KITU.*` extension set.
    pub fn register_default_extensions(&mut self) -> CalcResult<()> {
        register_kitu_examples(&mut self.registry)
    }

    /// Evaluate a function after the owning Kitu layer has resolved arguments.
    pub fn evaluate(
        &self,
        name: &str,
        arguments: &[Argument],
        context: &mut dyn EvalContext,
    ) -> CalcResult {
        self.registry.evaluate(name, arguments, context)
    }

    /// Access compatibility metadata for a standard or installed extension.
    #[must_use]
    pub fn metadata(&self, name: &str) -> Option<&FunctionMetadata> {
        self.registry.metadata(name)
    }
}

impl Default for KituCalculationKernel {
    fn default() -> Self {
        Self::standard()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_and_kitu_namespaces_are_explicit() {
        let mut kernel = KituCalculationKernel::standard();
        assert!(kernel.metadata("SUM").is_some());
        assert!(kernel.metadata("KITU.CLAMP").is_none());

        kernel
            .register_default_extensions()
            .expect("known extension registration");
        let value = kernel
            .evaluate(
                "KITU.CLAMP",
                &[
                    Argument::scalar(12_i64),
                    Argument::scalar(0_i64),
                    Argument::scalar(10_i64),
                ],
                &mut PureContext,
            )
            .expect("Kitu extension calculation");
        assert_eq!(value, Value::from(10_i64));
    }

    #[test]
    fn standard_results_match_the_cross_product_contract() {
        let kernel = KituCalculationKernel::standard();
        let value = kernel
            .evaluate(
                "FLOOR",
                &[
                    Argument::scalar(Value::Number(
                        Number::try_from_f64(0.3).expect("finite fixture"),
                    )),
                    Argument::scalar(Value::Number(
                        Number::try_from_f64(0.1).expect("finite fixture"),
                    )),
                ],
                &mut PureContext,
            )
            .expect("standard calculation");
        let Value::Number(value) = value else {
            panic!("FLOOR must return a number");
        };
        assert_eq!(value.as_f64(), 0.3);
    }
}
