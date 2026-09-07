# kitu-calculation

Kitu's adapter for the version-pinned `openformula-kernel` crate. It exposes
the shared typed scalar/function contract and provides explicit registration of
product functions under the `KITU.*` namespace.

The crate does not parse formula source, resolve references, own runtime state,
or format results. Kitu runtime and scripting layers prepare arguments, inject
any clock/random capabilities, and map calculation errors into their own source
diagnostics.

`KituCalculationKernel::standard()` installs only the OpenFormula-derived
subset. Call `register_default_extensions()` to opt into `KITU.CLAMP` and
future versioned Kitu extensions.
