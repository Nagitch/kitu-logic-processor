import { browser } from "$app/environment";
import { env } from "$env/dynamic/public";
import { derived, get, writable } from "svelte/store";
import type {
  ActionValue,
  AppActionCatalog,
  AppActionDefinition,
  ClientOscMessage,
  DebugLogEntry,
  JsonOscArg,
  ServerEvent,
  WorldSnapshot,
} from "./types";
import { decodeKepEnvelope, encodeKepEnvelope, encodeOscPacket } from "./kep";

type ConnectionState = "idle" | "connecting" | "open" | "closed" | "error";
type WebTransportState =
  | "disabled"
  | "unsupported"
  | "connecting"
  | "ready"
  | "closed"
  | "error";
type OscSendStatus = {
  path: "none" | "webtransport" | "websocket-fallback" | "websocket";
  phase: "idle" | "pending" | "sent" | "fallback" | "failed";
  detail: string | null;
};
type WebTransportSession = {
  readonly ready: Promise<void>;
  readonly closed: Promise<unknown>;
  createBidirectionalStream(): Promise<WebTransportBidirectionalStream>;
  close(): void;
};
type WebTransportBidirectionalStream = {
  readable: ReadableStream<Uint8Array>;
  writable: WritableStream<Uint8Array>;
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
type WebTransportSendResult = {
  requestWritten: boolean;
  fallbackReason?: string;
};

const defaultSnapshot: WorldSnapshot = {
  tick: 0,
  objects: [],
};

export const connectionState = writable<ConnectionState>("idle");
export const worldSnapshot = writable<WorldSnapshot>(defaultSnapshot);
export const debugLogs = writable<DebugLogEntry[]>([]);
export const lastError = writable<string | null>(null);
export const appActionCatalog = writable<AppActionCatalog>({ actions: [] });
export const webTransportState = writable<WebTransportState>("disabled");
export const webTransportDetail = writable<string | null>(null);
export const lastOscSendStatus = writable<OscSendStatus>({
  path: "none",
  phase: "idle",
  detail: null,
});

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
    lastOscSendStatus.set({
      path: "webtransport",
      phase: "pending",
      detail: "opening stream",
    });
    sendOscOverWebTransport(webTransport, payload)
      .then((result) => {
        if (!result.requestWritten) {
          sendOscOverWebSocket(payload, {
            path: "websocket-fallback",
            detail: result.fallbackReason ?? "WebTransport pre-write failure",
          });
        }
      })
      .catch((error: unknown) => {
        const message =
          error instanceof Error
            ? error.message
            : `WebTransport send failed: ${error}`;
        lastOscSendStatus.set({
          path: "webtransport",
          phase: "failed",
          detail: message,
        });
        lastError.set(message);
      });
    return true;
  }

  return sendOscOverWebSocket(payload, {
    path: "websocket",
    detail: webTransportBypassReason(),
  });
}

function sendOscOverWebSocket(
  payload: ClientOscMessage,
  status: Pick<OscSendStatus, "path" | "detail">,
) {
  if (!socket || socket.readyState !== WebSocket.OPEN) {
    lastOscSendStatus.set({
      path: status.path,
      phase: "failed",
      detail: "WebSocket is not connected",
    });
    lastError.set("WebSocket is not connected");
    return false;
  }

  socket.send(JSON.stringify(payload));
  lastOscSendStatus.set({
    path: status.path,
    phase: status.path === "websocket-fallback" ? "fallback" : "sent",
    detail: status.detail,
  });
  lastError.set(null);
  return true;
}

async function sendOscOverWebTransport(
  session: WebTransportSession,
  payload: ClientOscMessage,
): Promise<WebTransportSendResult> {
  let stream: WebTransportBidirectionalStream;
  try {
    stream = await session.createBidirectionalStream();
  } catch (error) {
    return {
      requestWritten: false,
      fallbackReason:
        error instanceof Error
          ? `Stream unavailable: ${error.message}`
          : `Stream unavailable: ${error}`,
    };
  }
  const writer = stream.writable.getWriter();
  const oscPacket = encodeOscPacket(payload);
  const envelope = encodeKepEnvelope({
    payloadType: "osc",
    route: env.PUBLIC_KITU_ADMIN_KEP_ROUTE ?? "/room/main",
    flags: 0,
    payload: oscPacket,
  });

  try {
    try {
      await writer.write(envelope);
    } catch (error) {
      return {
        requestWritten: false,
        fallbackReason:
          error instanceof Error
            ? `Write unavailable: ${error.message}`
            : `Write unavailable: ${error}`,
      };
    }
    try {
      await writer.close();
      const response = await readStreamBytes(stream.readable);
      if (response.length > 0) {
        applyKepServerEvent(response);
      }
    } catch (error) {
      throw new Error(
        error instanceof Error
          ? `WebTransport post-write failed: ${error.message}`
          : `WebTransport post-write failed: ${error}`,
      );
    }
    lastOscSendStatus.set({
      path: "webtransport",
      phase: "sent",
      detail: "response applied",
    });
    lastError.set(null);
    return { requestWritten: true };
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
    const action = await appActionDefinition(actionId);
    const osc = materializeAppAction(action, inputs);
    return sendOsc(osc);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    lastOscSendStatus.set({
      path: "none",
      phase: "failed",
      detail: message,
    });
    lastError.set(message);
    return false;
  }
}

async function appActionDefinition(actionId: string) {
  const cached = get(appActionCatalog).actions.find(
    (action) => action.id === actionId,
  );
  if (cached) return cached;

  const response = await fetch(
    `${apiBaseUrl()}/app-actions/${encodeURIComponent(actionId)}`,
  );
  if (!response.ok) {
    const error = (await response.json().catch(() => null)) as {
      error?: string;
    } | null;
    throw new Error(error?.error ?? `App action failed: ${response.status}`);
  }
  return (await response.json()) as AppActionDefinition;
}

function materializeAppAction(
  action: AppActionDefinition,
  inputs: Record<string, ActionValue>,
): ClientOscMessage {
  const normalizedInputs = new Map<string, ActionValue>();
  for (const input of action.inputs) {
    const value = inputs[input.name] ?? input.default;
    if (!value) {
      if (input.required) {
        throw new Error(`Missing app action input: ${input.name}`);
      }
      continue;
    }
    normalizedInputs.set(input.name, coerceActionValue(input.name, value));
  }

  return {
    address: action.output.address,
    args: action.output.args.map((arg) => {
      if (arg.type === "literal") return actionValueToOscArg(arg.value);
      const value = normalizedInputs.get(arg.name);
      if (!value) throw new Error(`Missing app action input: ${arg.name}`);
      return actionValueToOscArg(value);
    }),
  };
}

function coerceActionValue(name: string, value: ActionValue): ActionValue {
  switch (value.type) {
    case "float": {
      const numberValue = Number(value.value);
      if (!Number.isFinite(numberValue)) {
        throw new Error(`Invalid app action input: ${name}`);
      }
      return { type: "float", value: numberValue };
    }
    case "int": {
      const numberValue = Number(value.value);
      if (!Number.isInteger(numberValue)) {
        throw new Error(`Invalid app action input: ${name}`);
      }
      return { type: "int", value: numberValue };
    }
    case "bool":
      return { type: "bool", value: Boolean(value.value) };
    case "string":
      return { type: "string", value: String(value.value) };
  }
}

function actionValueToOscArg(value: ActionValue): JsonOscArg {
  switch (value.type) {
    case "float":
      return { type: "float", value: value.value };
    case "int":
      return { type: "int", value: value.value };
    case "bool":
      return { type: "bool", value: value.value };
    case "string":
      return { type: "str", value: value.value };
  }
}

function apiBaseUrl() {
  return env.PUBLIC_KITU_ADMIN_API_URL ?? "http://localhost:8787";
}

function connectAdminWebTransport() {
  const url = env.PUBLIC_KITU_ADMIN_WT_URL;
  if (!browser || webTransport) return;
  if (!url) {
    webTransportState.set("disabled");
    webTransportDetail.set("PUBLIC_KITU_ADMIN_WT_URL is not set");
    return;
  }

  const Transport = (
    window as Window & { WebTransport?: WebTransportConstructor }
  ).WebTransport;
  if (!Transport) {
    webTransportState.set("unsupported");
    webTransportDetail.set("window.WebTransport is unavailable");
    return;
  }

  const options = webTransportOptions();
  if (options === null) return;
  webTransportState.set("connecting");
  webTransportDetail.set(url);
  const session = options ? new Transport(url, options) : new Transport(url);
  webTransport = session;

  session.ready
    .then(() => {
      webTransportReady = true;
      webTransportState.set("ready");
      webTransportDetail.set(url);
    })
    .catch((error: unknown) => {
      webTransportReady = false;
      webTransport = null;
      webTransportState.set("error");
      webTransportDetail.set(
        error instanceof Error ? error.message : String(error),
      );
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
        webTransportState.set("closed");
        webTransportDetail.set("session closed");
      }
    });
}

function webTransportOptions(): WebTransportOptions | undefined | null {
  const rawCertificateHash = env.PUBLIC_KITU_ADMIN_WT_CERT_SHA256;
  if (!rawCertificateHash) return undefined;
  const certificateHash = parseHexSha256(rawCertificateHash);
  if (!certificateHash) {
    webTransportState.set("error");
    webTransportDetail.set("certificate SHA-256 must be 64 hex chars");
    return null;
  }

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

function webTransportBypassReason() {
  const state = get(webTransportState);
  switch (state) {
    case "disabled":
      return "WebTransport disabled";
    case "unsupported":
      return "WebTransport unsupported";
    case "connecting":
      return "WebTransport connecting";
    case "closed":
      return "WebTransport closed";
    case "error":
      return "WebTransport unavailable";
    case "ready":
      return "WebTransport not ready";
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
