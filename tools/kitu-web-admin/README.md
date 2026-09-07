# Kitu Web Admin

Browser tools for observing Endless Arena, editing its next-run configuration
and controlling replay, alongside the generic Kitu World editor and logs.

The [delivery matrix](../../doc/verification/arena-delivery/README.md) records
the actual Arena scope and evidence. The shared
[verification recipe](../../doc/specs/arena-build-verification.md) includes
frontend tests, type/lint checks and the complete Rust/WASM plus Vite build.

## Layout

- `package/`: reusable `@kitu/admin` Svelte package with the Admin client,
  shell, controls, standard pages, CSS, and OSC-IR WASM loader contract.
- `frontend/`: the Arena SvelteKit application. It owns Arena routes,
  navigation, endpoint environment variables, titles, and world appearance.
- `starter/`: minimal independent SvelteKit consumer for validating the package
  against a neutral Kitu Admin host.

The demo backend is now owned by `apps/demo-game`, because it is an application
that consumes the Kitu framework crates rather than a reusable Web Admin tool.

The package and consumers use Tailwind CSS 4 through the Vite plugin. Shared
theme tokens and package source discovery live in `package/src/styles.css`.

## Run

The frontend uses the shared Rust OSC-IR model through a generated WASM package.
Local development requires the demo-game admin host, Rust, the
`wasm32-unknown-unknown` target, and the frontend pnpm dependencies before Vite
starts:

```sh
cargo run -p kitu-demo-game --bin kitu-demo-game-admin-host
```

In another shell, build the package before installing either consumer:

```sh
rustup target add wasm32-unknown-unknown
cd tools/kitu-web-admin/package
pnpm install --frozen-lockfile
pnpm run check
pnpm run test
pnpm run build

cd tools/kitu-web-admin/frontend
pnpm install --frozen-lockfile
pnpm run wasm
pnpm run dev
```

`pnpm run dev` and `pnpm run build` both run the WASM generation step first. The
generated package is written to `frontend/static/kitu-osc-ir-wasm/` and is not
committed.

The WASM package script accepts `KITU_OSC_IR_WASM_CRATE` for a pinned external
Kitu checkout and `KITU_ADMIN_WASM_OUT_DIR` for a consuming application's public
directory. `createOscIrLoader` accepts an explicit router base URL, module URL,
and WASM URL, so deployments below a path such as `/demo/` do not depend on the
browser's current nested route.

```sh
docker compose -f apps/demo-game/docker-compose.yml up --build
```

For browser WebTransport testing, first generate a short-lived development
certificate and certificate hash:

```sh
tools/kitu-webtransport-gateway/scripts/generate-dev-cert-in-docker.sh
tools/kitu-webtransport-gateway/scripts/check-dev-cert-in-docker.sh
```

To run the local WebTransport gateway smoke test:

```sh
tools/kitu-webtransport-gateway/scripts/smoke-in-docker.sh
```

Then open:

- Web Admin: http://localhost:5173
- Demo game admin host health: http://localhost:8787/health
- Experimental WebTransport gateway: https://localhost:9443 over UDP

The separate World page sends JSON-wrapped OSC-IR messages over WebSocket to
create and move generic logical objects. The backend broadcasts their world
snapshots and debug logs; Arena Inspector uses its own coherent application
inspection endpoint instead of that generic world tick.
When `PUBLIC_KITU_ADMIN_WT_URL` is configured, browser OSC sends can use the
experimental WebTransport gateway with KEP MessagePack envelopes. The existing
WebSocket connection remains the fallback and the source of state/log events.
The gateway compose service builds from
`tools/kitu-webtransport-gateway/Dockerfile`, which uses the repository's
current Rust 1.96 toolchain.
In Docker Compose, the frontend image includes Node 24 and Rust 1.96 with the
WASM target. The frontend service runs `pnpm install` and `pnpm run dev`; the
`predev` script generates the OSC-IR WASM package before Vite starts.

## Inspect Endless Arena

Open **Project → Arena Inspector**. Its endpoint, session, Live/Replay mode, run,
tick and simulation count identify the observation shared by every panel.
Select an entity from the minimap or keyboard-accessible list to inspect its
authoritative fields. Timeline rows show actual cue offsets, event counts and
values; the event window shows observed game events and command receipts. Timing
is the measured host owner-update cost, including in-lock bookkeeping, displayed
in milliseconds (the API uses microseconds). It is not rendering FPS or elapsed
game time.

Inspector refreshes while visible without claiming a game controller or
advancing a tick. All panels adopt one validated snapshot together, and failures
leave the last snapshot visibly stale. Use **Arena Replay** to save/import/load
recordings, then use Inspector's playback controls to step or seek to an exact
tick. A command acknowledgment is followed by a refreshed observation; returning
to live preserves the explicit-resume rule. **Game Parameters**, **Game Scripts**
and **Story Sequencing** retain their separate validate/apply-to-next-run flows.

To inspect the macOS standalone's embedded Runtime, launch it with its local
bridge enabled, then point both Admin endpoints at that bridge. From the frontend
directory:

```sh
PUBLIC_KITU_ADMIN_API_URL=http://127.0.0.1:8789 \
PUBLIC_KITU_ADMIN_WS_URL=ws://127.0.0.1:8789/ws \
  pnpm run dev
```

The default standalone-server endpoints are `http://localhost:8787` and
`ws://localhost:8787/ws`. These are Admin connections; Arena's Unity controller
uses the separate `/ws/arena` route. The
[inspection contract](../../doc/specs/arena-inspection.md) describes snapshot
coherence, exact wide integers, history bounds and timing/replay semantics.
