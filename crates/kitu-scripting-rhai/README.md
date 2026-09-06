# kitu-scripting-rhai

A bounded Rhai function host for Kitu applications. The former in-memory script
registry placeholder is replaced by real compilation and evaluation using pinned
Rhai 1.26.0. The crate provides no application state or Runtime callbacks.

## Contract

```rust
use kitu_scripting_rhai::{Limits, ScriptHost};
use serde_json::json;

let host = ScriptHost::new(Limits::default())?;
let program = host.compile(r#"
    fn boss(input) {
        if input.timerExpired { #{action: "telegraph", duration: 0.8} }
        else { #{action: "wait", duration: 0.0} }
    }
"#)?;
let request = host.invoke(&program, "boss", &json!({"timerExpired": true}))?;
# Ok::<(), kitu_scripting_rhai::Diagnostic>(())
```

- `ScriptHost::new(Limits)` validates all limits before constructing its engine.
- `compile(source)` produces an opaque, cloneable `CompiledScript`. Compilation
  never executes source: optimization is disabled.
- `invoke(program, entrypoint, input)` calls one script function with one copied
  JSON argument. Each invocation gets a fresh scope and operation counter,
  including execution of top-level code. Hosts and programs are `Send + Sync`.
- Script mutation affects only its copy. Applications validate returned requests
  and apply game changes themselves, after a successful invocation.
- A program may be used by another host only with identical limits.
- `Diagnostic` is cloneable, serializable and implements `Display`/`Error`.
  Its JSON fields are `kind`, `message`, `line` and `column`. Generated messages
  are capped at 1,024 UTF-8 bytes; known source positions are one-based.

JSON input/output retains null, booleans, signed 64-bit integers, finite binary64
numbers, strings, arrays and objects. Unsigned integers above `i64::MAX`,
non-finite results, characters, ranges, function pointers and other Rhai types
are rejected, without silent conversion to null or strings. Object key order has
no gameplay meaning. Encoded byte limits include JSON escaping and punctuation.

## Execution policy

`POLICY_VERSION = "kitu-rhai-json-v1"` and `RHAI_VERSION = "1.26.0"` identify the
host contract. The engine starts with `Engine::new_raw()` and registers only
`ArithmeticPackage` and `LogicPackage`. No standard, language-core, string,
iterator, function, time or application package is installed. Basic syntax
includes conditionals, loops, named functions, map/array literals and indexing.

Imports, exports, dynamic evaluation, print/debug calls, function-pointer calls
and closures are disabled. The raw engine has no module resolver. No filesystem,
network, clock, random, sleep or logging bindings exist.

The Rhai dependency enables only its `sync` and `serde` additions. It does not
change shared numeric semantics with features such as `f32_float`, or remove
language facilities globally with `no_*` flags; Tanu can continue using its own
separate engine. Its existing `no_module`/`no_closure` features are supported;
the host does not depend on the optional resolver or shared-value inspection APIs.
Downstream builds must not enable incompatible Rhai features
such as `unchecked`, `no_float` or `f32_float`. The locked dependency graph and
execution build belong in compatibility/replay evidence.

## Default limits

| Resource | Default |
| --- | ---: |
| Source UTF-8 bytes | 65,536 |
| Encoded input / output JSON bytes | 16,384 each |
| Rhai operations per invocation | 10,000 |
| Script call / expression depth | 32 each |
| Functions per compilation | 64 |
| Variables per scope | 128 |
| String / key bytes | 4,096 |
| Array / object entries | 256 / 128 |
| JSON depth / value count | 16 / 4,096 |

Every bound is nonzero and has a documented ceiling in `Limits`; zero is rejected.
The engine also aggregates nested array entries, nested map entries and string
contents when enforcing its intermediate-data limits. JSON validation separately
bounds depth, value count, keys and final encoded bytes before returning a value.

Operation counts bound interpreter work; they are not elapsed-time, allocator or
process quotas. Only the small reviewed arithmetic/comparison package set is
registered, with no blocking native callbacks. Applications should compile and
probe candidates outside the simulation lock, retain the last valid version on
failure, and pin source plus engine/policy/build identity for old replay. The
host does not provide file watching, candidate activation, game fallback rules,
cross-version replay migration or a process isolation boundary. Floating-point
reproducibility requires the recorded execution compatibility policy.

## Verification

Real-engine tests cover boss decisions, JSON scalar fidelity, caller isolation,
concurrent use, compile-time non-execution, forbidden capabilities, bounded
diagnostics, source/expression/function limits, loops/recursion/variable/data
growth, escaped byte boundaries, JSON depth/nodes/keys and non-finite output.

Run in the development container:

```sh
cargo test --locked -p kitu-scripting-rhai
cargo clippy --locked -p kitu-scripting-rhai --all-targets --all-features -- -D warnings
RUSTDOCFLAGS=-Dwarnings cargo doc --locked -p kitu-scripting-rhai --no-deps --all-features
```

This crate remains internal (`publish = false`). Application boss rules and
their next-run/replay policy live in `apps/demo-game`.
