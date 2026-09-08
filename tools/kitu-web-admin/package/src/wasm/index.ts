import type { ClientOscMessage, JsonOscArg } from '../client/types.js'

export interface OscIrWasmModule {
  default(input?: string | URL | Request): Promise<unknown>
  admin_world_spawn(kind: string, x: number, y: number, z: number): unknown
  admin_world_move(id: string, x: number, y: number, z: number): unknown
  admin_world_reset(): unknown
}

export interface OscIrLoaderOptions {
  /** Application base URL, normally derived from the consuming router's base path. */
  baseUrl?: string | URL
  /** URL to the generated JS module. Relative URLs resolve against baseUrl. */
  moduleUrl?: string | URL
  /** Optional explicit .wasm URL passed to wasm-bindgen's initializer. */
  wasmUrl?: string | URL
  importModule?: (url: string) => Promise<unknown>
}

export function createOscIrLoader(options: OscIrLoaderOptions = {}) {
  let modulePromise: Promise<OscIrWasmModule> | null = null
  const dynamicImport = options.importModule ?? importRuntimeModule
  const documentBase = typeof document === 'undefined' ? 'http://localhost/' : document.baseURI
  const baseUrl = options.baseUrl instanceof URL ? options.baseUrl.href : new URL(options.baseUrl ?? '.', documentBase).href
  const resolve = (value: string | URL) => (value instanceof URL ? value.href : new URL(value, baseUrl).href)
  const moduleUrl = resolve(options.moduleUrl ?? 'kitu-osc-ir-wasm/kitu_osc_ir_wasm.js')
  const wasmUrl = options.wasmUrl ? resolve(options.wasmUrl) : undefined
  async function load() {
    modulePromise ??= dynamicImport(moduleUrl)
      .then(async value => {
        const wasm = value as OscIrWasmModule
        await wasm.default(wasmUrl)
        return wasm
      })
      .catch((error: unknown) => {
        modulePromise = null
        throw new Error(`OSC-IR WASM bindings are unavailable: ${error instanceof Error ? error.message : String(error)}`)
      })
    return modulePromise
  }
  return {
    initialize: async () => {
      await load()
    },
    spawn: async (kind: string, x: number, y: number, z: number) => validate((await load()).admin_world_spawn(kind, x, y, z)),
    move: async (id: string, x: number, y: number, z: number) => validate((await load()).admin_world_move(id, x, y, z)),
    reset: async () => validate((await load()).admin_world_reset())
  }
}

async function importRuntimeModule(url: string) {
  return import(/* @vite-ignore */ url) as Promise<unknown>
}

function validate(value: unknown): ClientOscMessage {
  if (!record(value) || typeof value.address !== 'string' || !Array.isArray(value.args) || !value.args.every(isArg))
    throw new Error('OSC-IR WASM returned an invalid message')
  return { address: value.address, args: value.args }
}
function isArg(value: unknown): value is JsonOscArg {
  if (!record(value) || typeof value.type !== 'string') return false
  if (['int', 'int64', 'float'].includes(value.type)) return typeof value.value === 'number'
  if (value.type === 'str') return typeof value.value === 'string'
  return value.type === 'bool' && typeof value.value === 'boolean'
}
function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null
}
