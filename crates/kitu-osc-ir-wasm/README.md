# kitu-osc-ir-wasm

WASM bindings for the shared `kitu-osc-ir` message model.

The crate exposes browser-facing builders for the Web Admin's JSON WebSocket
transport while keeping the canonical message construction in Rust. Generate the
frontend package with:

The shared Admin package owns the repeatable browser build entrypoint:

```sh
pnpm --dir tools/kitu-web-admin/package run wasm
```

It writes to `tools/kitu-web-admin/package/public/kitu-osc-ir-wasm` by default.
Set `KITU_ADMIN_WASM_OUT_DIR` to an absolute consumer public directory when
packaging an application.

The generated package is intentionally not committed. The Web Admin loads it
from `/kitu-osc-ir-wasm/kitu_osc_ir_wasm.js` at runtime.
