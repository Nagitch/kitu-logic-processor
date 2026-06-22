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
import { decodeKepEnvelope, encodeKepEnvelope, encodeOscPacket } from "./kep";

type ConnectionState = "idle" | "connecting" | "open" | "closed" | "error";
type WebTransportSession = {
  readonly ready: Promise<void>;
  readonly closed: Promise<unknown>;
  createBidirectionalStream(): Promise<{
    readable: ReadableStream<Uint8Array>;
    writable: WritableStream<Uint8Array>;
  }>;
  close(): void;
};
type WebTransportOptions = {
  serverCertificateHashes?: Array<{
    algorithm: "sha-256";
    value: Uint8Array;
  }>;
};
type WebTransportConstructor = new (
  url: string,
  options?: WebTransportOptions,
) => WebTransportSession;

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
let webTransport: WebTransportSession | null = null;
let webTransportReady = false;
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
    connectAdminWebTransport();
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
  if (webTransportReady && webTransport) {
    sendOscOverWebTransport(webTransport, payload).catch((error: unknown) => {
      lastError.set(
        error instanceof Error
          ? error.message
          : `WebTransport send failed: ${error}`,
      );
      sendOscOverWebSocket(payload);
    });
    return true;
  }

  return sendOscOverWebSocket(payload);
}

function sendOscOverWebSocket(payload: ClientOscMessage) {
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    lastError.set("WebSocket is not connected");
    return false;
  }

  socket.send(JSON.stringify(payload));
  return true;
}

async function sendOscOverWebTransport(
  session: WebTransportSession,
  payload: ClientOscMessage,
) {
  const stream = await session.createBidirectionalStream();
  const writer = stream.writable.getWriter();
  const oscPacket = encodeOscPacket(payload);
  const envelope = encodeKepEnvelope({
    payloadType: "osc",
    route: env.PUBLIC_KITU_ADMIN_KEP_ROUTE ?? "/room/main",
    flags: 0,
    payload: oscPacket,
  });

  try {
    await writer.write(envelope);
    await writer.close();
    const response = await readStreamBytes(stream.readable);
    if (response.length > 0) {
      applyKepServerEvent(response);
    }
    lastError.set(null);
  } finally {
    writer.releaseLock();
  }
}

async function readStreamBytes(stream: ReadableStream<Uint8Array>) {
  const reader = stream.getReader();
  const chunks: Uint8Array[] = [];
  let length = 0;

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
      length += value.length;
    }
  } finally {
    reader.releaseLock();
  }

  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  return bytes;
}

function applyKepServerEvent(bytes: Uint8Array) {
  const envelope = decodeKepEnvelope(bytes);
  if (envelope.payloadType !== "json") {
    throw new Error(
      `Unsupported KEP response payload: ${envelope.payloadType}`,
    );
  }

  const json = new TextDecoder().decode(envelope.payload);
  applyServerEvent(JSON.parse(json) as ServerEvent);
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

function connectAdminWebTransport() {
  const url = env.PUBLIC_KITU_ADMIN_WT_URL;
  if (!browser || !url || webTransport) return;

  const Transport = (
    window as Window & { WebTransport?: WebTransportConstructor }
  ).WebTransport;
  if (!Transport) {
    return;
  }

  const options = webTransportOptions();
  const session = options ? new Transport(url, options) : new Transport(url);
  webTransport = session;

  session.ready
    .then(() => {
      webTransportReady = true;
    })
    .catch((error: unknown) => {
      webTransportReady = false;
      webTransport = null;
      lastError.set(
        error instanceof Error
          ? `WebTransport connection failed: ${error.message}`
          : `WebTransport connection failed: ${error}`,
      );
    });

  session.closed
    .catch(() => null)
    .finally(() => {
      if (webTransport === session) {
        webTransportReady = false;
        webTransport = null;
      }
    });
}

function webTransportOptions(): WebTransportOptions | undefined {
  const certificateHash = parseHexSha256(env.PUBLIC_KITU_ADMIN_WT_CERT_SHA256);
  if (!certificateHash) return undefined;

  return {
    serverCertificateHashes: [
      {
        algorithm: "sha-256",
        value: certificateHash,
      },
    ],
  };
}

function parseHexSha256(value: string | undefined) {
  if (!value) return null;

  const normalized = value.replace(/[^a-fA-F0-9]/g, "");
  if (normalized.length !== 64) {
    lastError.set("WebTransport certificate SHA-256 must be 64 hex chars");
    return null;
  }

  const bytes = new Uint8Array(32);
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = Number.parseInt(
      normalized.slice(index * 2, index * 2 + 2),
      16,
    );
  }
  return bytes;
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
