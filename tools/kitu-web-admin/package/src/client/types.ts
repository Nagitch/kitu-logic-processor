export type ConnectionState = 'idle' | 'connecting' | 'open' | 'closed' | 'error'
export type WebTransportState = 'disabled' | 'unsupported' | 'connecting' | 'ready' | 'closed' | 'error'
export type OscSendStatus = {
  path: 'none' | 'http' | 'webtransport' | 'websocket-fallback' | 'websocket'
  phase: 'idle' | 'pending' | 'sent' | 'applied' | 'fallback' | 'failed'
  detail: string | null
}
export type WorldObject = { id: string; kind: string; x: number; y: number; z: number; color: string }
export type WorldSnapshot = { tick: number; objects: WorldObject[] }
export type DebugLogEntry = {
  id: number
  level: 'info' | 'warn' | 'error'
  message: string
  oscAddress?: string | null
  tick: number
}
export type JsonOscArg =
  { type: 'int' | 'int64' | 'float'; value: number } | { type: 'str'; value: string } | { type: 'bool'; value: boolean }
export type ClientOscMessage = { address: string; args: JsonOscArg[] }
export type ActionValue =
  { type: 'string'; value: string } | { type: 'float' | 'int'; value: number } | { type: 'bool'; value: boolean }
export type AppActionScope = { type: 'kitu-general' } | { type: 'project'; appId: string }
export type ActionInputSpec = {
  name: string
  label: string
  valueType: 'string' | 'float' | 'int' | 'bool'
  required: boolean
  default?: ActionValue | null
}
export type AppActionDefinition = {
  id: string
  scope: AppActionScope
  label: string
  description?: string | null
  cli: { command: string }
  ui: { kind: 'form' | 'button'; submitLabel: string; destructive: boolean }
  inputs: ActionInputSpec[]
  output: { address: string; args: Array<{ type: 'input'; name: string } | { type: 'literal'; value: ActionValue }> }
}
export type AppActionCatalog = { actions: AppActionDefinition[] }
export type ActionRunResponse = { actionId: string; osc: ClientOscMessage; snapshot: WorldSnapshot }
export type ServerEvent =
  | { type: 'connected'; protocol: string; tick: number }
  | { type: 'state'; snapshot: WorldSnapshot }
  | { type: 'log'; entry: DebugLogEntry }
  | { type: 'osc'; address: string; args: JsonOscArg[] }
  | { type: 'error'; message: string }

export interface WebSocketLike {
  readonly readyState: number
  addEventListener(type: 'open' | 'close' | 'error' | 'message', listener: (event: Event | MessageEvent) => void): void
  send(data: string): void
  close(): void
}

export interface WebTransportSessionLike {
  readonly ready: Promise<void>
  readonly closed: Promise<unknown>
  createBidirectionalStream(): Promise<{ readable: ReadableStream<Uint8Array>; writable: WritableStream<Uint8Array> }>
  close(): void
}

export interface AdminClientOptions {
  apiUrl: string
  webSocketUrl: string
  webTransportUrl?: string
  webTransportCertificateSha256?: string
  kepRoute?: string
  reconnectDelayMs?: number
  fetch?: typeof globalThis.fetch
  createWebSocket?: (url: string) => WebSocketLike
  createWebTransport?: (url: string, certificateSha256?: Uint8Array) => WebTransportSessionLike
  isBrowser?: boolean
}
