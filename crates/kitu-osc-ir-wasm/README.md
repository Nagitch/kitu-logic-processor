# kitu-osc-ir-wasm

WASM bindings for the shared `kitu-osc-ir` message model.

The crate exposes browser-facing builders for the Web Admin's JSON WebSocket
transport while keeping the canonical message construction in Rust. Generate the
frontend package with:

```sh
wasm-pack build crates/kitu-osc-ir-wasm \
  --target web \
  --out-dir ../../tools/kitu-web-admin/frontend/static/kitu-osc-ir-wasm \
  --out-name kitu_osc_ir_wasm
```

The generated package is intentionally not committed. The Web Admin loads it
from `/kitu-osc-ir-wasm/kitu_osc_ir_wasm.js` at runtime.
