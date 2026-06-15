import { browser } from "$app/environment";
import { env } from "$env/dynamic/public";
import { derived, get, writable } from "svelte/store";
import type {
  ActionRunResponse,
  ActionValue,
  AppActionCatalog,
  ClientOscMessage,
  DebugLogEntry,
  ServerEvent,
  WorldSnapshot,
} from "./types";

type ConnectionState = "idle" | "connecting" | "open" | "closed" | "error";

const defaultSnapshot: WorldSnapshot = {
  tick: 0,
  objects: [],
};

export const connectionState = writable<ConnectionState>("idle");
export const worldSnapshot = writable<WorldSnapshot>(defaultSnapshot);
export const debugLogs = writable<DebugLogEntry[]>([]);
export const lastError = writable<string | null>(null);
export const appActionCatalog = writable<AppActionCatalog>({ actions: [] });

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
    loadAppActions();
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
  return runAppAction("spawn-object", {
    kind: { type: "string", value: kind },
    x: { type: "float", value: x },
    y: { type: "float", value: y },
    z: { type: "float", value: z },
  });
}

export async function moveObject(id: string, x: number, y: number, z: number) {
  return runAppAction("move-object", {
    id: { type: "string", value: id },
    x: { type: "float", value: x },
    y: { type: "float", value: y },
    z: { type: "float", value: z },
  });
}

export async function resetWorld() {
  return runAppAction("reset-world", {});
}

export async function loadAppActions() {
  try {
    const response = await fetch(`${apiBaseUrl()}/app-actions`);
    if (!response.ok) {
      throw new Error(`App action catalog request failed: ${response.status}`);
    }
    const catalog = (await response.json()) as AppActionCatalog;
    appActionCatalog.set(catalog);
    return catalog;
  } catch (error) {
    lastError.set(error instanceof Error ? error.message : String(error));
    return null;
  }
}

export async function runAppAction(
  actionId: string,
  inputs: Record<string, ActionValue>,
) {
  try {
    const response = await fetch(
      `${apiBaseUrl()}/app-actions/${encodeURIComponent(actionId)}/run`,
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ inputs }),
      },
    );
    if (!response.ok) {
      const error = (await response.json().catch(() => null)) as {
        error?: string;
      } | null;
      throw new Error(error?.error ?? `App action failed: ${response.status}`);
    }
    const result = (await response.json()) as ActionRunResponse;
    worldSnapshot.set(result.snapshot);
    lastError.set(null);
    return result;
  } catch (error) {
    lastError.set(error instanceof Error ? error.message : String(error));
    return null;
  }
}

function apiBaseUrl() {
  return env.PUBLIC_KITU_ADMIN_API_URL ?? "http://localhost:8787";
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
