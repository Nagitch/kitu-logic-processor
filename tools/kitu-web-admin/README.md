# Kitu Web Admin

Initial browser admin vertical slice for organizing and debugging a Kitu logic processor game without Unity.

## Layout

- `frontend/`: SvelteKit admin UI using local shadcn-svelte style components, Bits UI primitives, and Three.js.
- `docker-compose.yml`: Local development hosting for the admin UI and the `apps/demo-game` admin host.

The demo backend is now owned by `apps/demo-game`, because it is an application
that consumes the Kitu framework crates rather than a reusable Web Admin tool.

## Run

The frontend uses the shared Rust OSC-IR model through a generated WASM package.
Local development requires the demo-game admin host, Rust, the
`wasm32-unknown-unknown` target, and the frontend pnpm dependencies before Vite
starts:

```sh
cargo run -p kitu-demo-game --bin kitu-demo-game-admin-host
```

In another shell:

```sh
rustup target add wasm32-unknown-unknown
cd tools/kitu-web-admin/frontend
pnpm install
pnpm run wasm
pnpm run dev
```

`pnpm run dev` and `pnpm run build` both run the WASM generation step first. The
generated package is written to `frontend/static/kitu-osc-ir-wasm/` and is not
committed.

```sh
docker compose -f tools/kitu-web-admin/docker-compose.yml up --build
```

For browser WebTransport testing, first generate a short-lived development
certificate and certificate hash:

```sh
tools/kitu-webtransport-gateway/scripts/generate-dev-cert-in-docker.sh
```

To run the local WebTransport gateway smoke test:

```sh
tools/kitu-webtransport-gateway/scripts/smoke-in-docker.sh
```

Then open:

- Web Admin: http://localhost:5173
- Demo game admin host health: http://localhost:8787/health
- Experimental WebTransport gateway: https://localhost:9443 over UDP

The Web Admin sends JSON-wrapped OSC-IR messages over WebSocket to create and move logical world objects. The backend logs inbound admin commands, ticks the Kitu runtime, and broadcasts world snapshots and debug logs back to the browser.
When `PUBLIC_KITU_ADMIN_WT_URL` is configured, browser OSC sends can use the
experimental WebTransport gateway with KEP MessagePack envelopes. The existing
WebSocket connection remains the fallback and the source of state/log events.
The gateway compose service builds from
`tools/kitu-webtransport-gateway/Dockerfile`, which intentionally uses Rust
1.88 because the WebTransport crate currently requires it.
In Docker Compose, the frontend image includes Node 22 and Rust 1.82 with the
WASM target. The frontend service runs `pnpm install` and `pnpm run dev`; the
`predev` script generates the OSC-IR WASM package before Vite starts.
