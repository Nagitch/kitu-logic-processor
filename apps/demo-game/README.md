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

## Scenario tests

The app scenarios are ordinary Rust tests and run in the workspace CI:

```sh
cargo test -p kitu-demo-game
```
