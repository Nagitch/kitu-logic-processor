# Endless Arena native library

This application-owned crate builds `kitu_demo_game_native` as a dynamic library,
static library and Rust library. It creates the same complete Arena Runtime as
the server. The generic ABI and C header live in `kitu-unity-ffi`; no Arena rules
are duplicated in the native crate.

Create with ABI 1 and empty configuration to use the embedded, real Tanu TMD.
Alternatively pass `{"contractVersion":1,"content":<ContentVersion>}` with a
previously evaluated, hashed configuration. Invalid values, hashes, unknown
fields and incompatible contracts are rejected before returning a handle.

The caller owns the one 60 Hz scheduler, submits typed input without advancing
time, ticks once and retrieves the complete output batch. Inspect is read-only.
A short read does not consume output; the next tick is refused until the previous
batch is retrieved. Destroy on the owning thread exactly once.

See [`arena-native-abi.md`](../../../doc/specs/arena-native-abi.md) for the complete
contract, reproducible builds and full-scenario C verification. Unity packaging
and the shared development Admin/CLI bridge follow in stage 11.
