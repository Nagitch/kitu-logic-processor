# Kitu Web Admin

Initial browser admin vertical slice for organizing and debugging a Kitu logic processor game without Unity.

## Layout

- `frontend/`: SvelteKit admin UI using local shadcn-svelte style components, Bits UI primitives, and Three.js.
- `backend/`: Small Rust backend app using Kitu workspace crates and exposing OSC-IR style messages over WebSocket.
- `docker-compose.yml`: Local development hosting for the admin UI and backend.

## Run

```sh
docker compose -f tools/kitu-web-admin/docker-compose.yml up --build
```

Then open:

- Web Admin: http://localhost:5173
- Backend health: http://localhost:8787/health

The Web Admin sends JSON-wrapped OSC-IR messages over WebSocket to create and move logical world objects. The backend logs inbound admin commands, ticks the Kitu runtime, and broadcasts world snapshots and debug logs back to the browser.
