import { browser } from "$app/environment";
import { env } from "$env/dynamic/public";
import { derived, get, writable } from "svelte/store";
import type {
  ClientOscMessage,
  DebugLogEntry,
  ServerEvent,
  WorldSnapshot,
} from "./types";
import {
  buildAdminWorldMove,
  buildAdminWorldReset,
  buildAdminWorldSpawn,
  initializeOscIr,
} from "./osc-ir";

type ConnectionState = "idle" | "connecting" | "open" | "closed" | "error";

const defaultSnapshot: WorldSnapshot = {
  tick: 0,
  objects: [],
};

export const connectionState = writable<ConnectionState>("idle");
export const worldSnapshot = writable<WorldSnapshot>(defaultSnapshot);
export const debugLogs = writable<DebugLogEntry[]>([]);
export const lastError = writable<string | null>(null);

export const objectCount = derived(
  worldSnapshot,
  ($snapshot) => $snapshot.objects.length,
);

let socket: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

export function connectAdminSocket() {
  if (!browser) return;
  if (
    socket &&
    (socket.readyState === WebSocket.OPEN ||
      socket.readyState === WebSocket.CONNECTING)
  ) {
    return;
  }

  connectionState.set("connecting");
  socket = new WebSocket(
    env.PUBLIC_KITU_ADMIN_WS_URL ?? "ws://localhost:8787/ws",
  );

  socket.addEventListener("open", () => {
    connectionState.set("open");
    lastError.set(null);
    initializeOscIr().catch((error: unknown) => {
      lastError.set(error instanceof Error ? error.message : String(error));
    });
  });

  socket.addEventListener("message", (event) => {
    const parsed = JSON.parse(event.data) as ServerEvent;
    applyServerEvent(parsed);
  });

  socket.addEventListener("close", () => {
    connectionState.set("closed");
    socket = null;
    scheduleReconnect();
  });

  socket.addEventListener("error", () => {
    connectionState.set("error");
    lastError.set("WebSocket connection failed");
  });
}

export function sendOsc(payload: ClientOscMessage) {
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    lastError.set("WebSocket is not connected");
    return false;
  }

  socket.send(JSON.stringify(payload));
  return true;
}

export async function spawnObject(
  kind: string,
  x: number,
  y: number,
  z: number,
) {
  return sendBuiltMessage(() => buildAdminWorldSpawn(kind, x, y, z));
}

export async function moveObject(id: string, x: number, y: number, z: number) {
  return sendBuiltMessage(() => buildAdminWorldMove(id, x, y, z));
}

export async function resetWorld() {
  return sendBuiltMessage(buildAdminWorldReset);
}

async function sendBuiltMessage(build: () => Promise<ClientOscMessage>) {
  try {
    const payload = await build();
    return sendOsc(payload);
  } catch (error) {
    lastError.set(error instanceof Error ? error.message : String(error));
    return false;
  }
}

function applyServerEvent(event: ServerEvent) {
  switch (event.type) {
    case "connected": {
      const connectedEntry: DebugLogEntry = {
        id: Date.now(),
        level: "info",
        message: `connected ${event.protocol}`,
        oscAddress: null,
        tick: event.tick,
      };
      debugLogs.update((entries) => [connectedEntry, ...entries]);
      break;
    }
    case "state":
      worldSnapshot.set(event.snapshot);
      break;
    case "log":
      debugLogs.update((entries) =>
        [
          event.entry,
          ...entries.filter((entry) => entry.id !== event.entry.id),
        ].slice(0, 500),
      );
      break;
    case "osc": {
      const oscEntry: DebugLogEntry = {
        id: Date.now() + Math.random(),
        level: "info",
        message: `backend -> admin ${event.address}`,
        oscAddress: event.address,
        tick: get(worldSnapshot).tick,
      };
      debugLogs.update((entries) => [oscEntry, ...entries].slice(0, 500));
      break;
    }
    case "error": {
      lastError.set(event.message);
      const errorEntry: DebugLogEntry = {
        id: Date.now() + Math.random(),
        level: "error",
        message: event.message,
        oscAddress: null,
        tick: get(worldSnapshot).tick,
      };
      debugLogs.update((entries) => [errorEntry, ...entries].slice(0, 500));
      break;
    }
  }
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connectAdminSocket();
  }, 1200);
}
