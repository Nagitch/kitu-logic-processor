# Kitu Admin starter

Minimal independent SvelteKit consumer of `@kitu/admin`. Set
`PUBLIC_KITU_ADMIN_API_URL` and `PUBLIC_KITU_ADMIN_WS_URL` for a Kitu Admin host,
then run `pnpm install`, `pnpm run check`, and `pnpm run dev`.

The `predev` and `prebuild` hooks generate OSC-IR bindings into this app's
`static/kitu-osc-ir-wasm` directory. Set `KITU_OSC_IR_WASM_CRATE` when building
against a pinned Kitu checkout outside the package's default repository layout.
