# Kitu Demo Game

`apps/demo-game` is a small application built on top of the Kitu framework crates.
It exists for vertical-slice development, Web Admin hosting, and CI scenario tests.

## Layout

- `src/lib.rs`: app-level runtime construction and project action loading.
- `src/bin/admin_host.rs`: local HTTP/WebSocket host used by Web Admin.
- `kitu-app-actions.toml`: project-owned app action manifest.
- `scenarios/`: checked-in scenario fixtures for CI.
- `tests/`: scenario test harnesses that execute fixtures against the Kitu runtime.
- `docker-compose.yml`: standalone app host plus Web Admin frontend.

## Run

From the repository root:

```sh
cargo run -p kitu-demo-game --bin kitu-demo-game-admin-host
```

Then open the Web Admin frontend separately, or use the app compose file:

```sh
docker compose -f apps/demo-game/docker-compose.yml up --build
```

Endpoints:

- Web Admin: http://localhost:5173
- Demo game admin host: http://localhost:8787
- Health: http://localhost:8787/health
- Web Admin WebSocket: ws://localhost:8787/ws
- Unity/runtime WebSocket: ws://localhost:8787/ws/runtime
- Experimental WebTransport gateway: https://localhost:9443 over UDP

The `/ws/runtime` endpoint is the development-time Unity vertical slice. It
accepts OSC-IR JSON messages directly, including `/input/move`, advances the
same runtime used by the embedded path, and broadcasts `/render/player/transform`
responses for presentation clients. It also forwards world `state` snapshots so
Unity can mirror Web Admin object spawn/move/reset actions.

The WebTransport gateway is a separate local-development container. It receives
KEP MessagePack envelopes, decodes OSC packet payloads, and relays them to the
existing Web Admin WebSocket endpoint over the Docker internal network. The
existing WebSocket endpoints remain the fallback and comparison path.

## Scenario tests

The app scenarios are ordinary Rust tests and run in the workspace CI:

```sh
cargo test -p kitu-demo-game
```

## Endless Arena migration

The same host also runs the Arena application at 60 Hz independently of messages.
Open `Assets/KituDemoApp/EndlessArena/KituEndlessArena.unity` in the Unity demo
project and connect to `ws://localhost:8787/ws/runtime`. The default scene
plays the complete endless game, including boss rewards, results and retry. The
original `EndlessArena.unity` and frozen C# fixtures remain the comparison oracle.

WASD moves, mouse buttons fire the two weapons, Z/X use consumables, E opens a
nearby chest, I/Tab opens inventory, and Escape closes an overlay or pauses.
The client only submits input and renders server state. See the
[Arena contract](../../doc/specs/arena-runtime-contract.md) and
[full-game evidence](../../doc/verification/arena-progression/results.json).

To include the live migration scene in PlayMode validation, run the host and set
`KITU_ARENA_WS_URL=ws://localhost:8787/ws/runtime` when launching Unity tests.
Without that variable, the external-host connection test is explicitly skipped.
