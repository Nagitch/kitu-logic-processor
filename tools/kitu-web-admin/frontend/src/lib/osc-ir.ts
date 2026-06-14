import { browser } from "$app/environment";
import type { ClientOscMessage, JsonOscArg } from "./types";

type OscIrWasmModule = {
  default: () => Promise<unknown>;
  admin_world_spawn: (kind: string, x: number, y: number, z: number) => unknown;
  admin_world_move: (id: string, x: number, y: number, z: number) => unknown;
  admin_world_reset: () => unknown;
};

let wasmModule: Promise<OscIrWasmModule> | null = null;
const importRuntimeModule = new Function(
  "specifier",
  "return import(specifier)",
) as (specifier: string) => Promise<unknown>;

export async function initializeOscIr() {
  await loadOscIrWasm();
}

export async function buildAdminWorldSpawn(
  kind: string,
  x: number,
  y: number,
  z: number,
) {
  const wasm = await loadOscIrWasm();
  return assertClientOscMessage(wasm.admin_world_spawn(kind, x, y, z));
}

export async function buildAdminWorldMove(
  id: string,
  x: number,
  y: number,
  z: number,
) {
  const wasm = await loadOscIrWasm();
  return assertClientOscMessage(wasm.admin_world_move(id, x, y, z));
}

export async function buildAdminWorldReset() {
  const wasm = await loadOscIrWasm();
  return assertClientOscMessage(wasm.admin_world_reset());
}

async function loadOscIrWasm() {
  if (!browser) {
    throw new Error("OSC-IR WASM bindings can only be loaded in the browser");
  }

  wasmModule ??= importRuntimeModule("/kitu-osc-ir-wasm/kitu_osc_ir_wasm.js")
    .then(async (module) => {
      const wasm = module as OscIrWasmModule;
      await wasm.default();
      return wasm;
    })
    .catch((error: unknown) => {
      wasmModule = null;
      const detail = error instanceof Error ? error.message : String(error);
      throw new Error(
        `OSC-IR WASM bindings are not available. Run \`npm run wasm\` before starting Web Admin. ${detail}`,
      );
    });

  return wasmModule;
}

function assertClientOscMessage(value: unknown): ClientOscMessage {
  if (!isRecord(value) || typeof value.address !== "string") {
    throw new Error("OSC-IR WASM returned an invalid message");
  }

  const args = Array.isArray(value.args) ? value.args : [];
  if (!args.every(isJsonOscArg)) {
    throw new Error("OSC-IR WASM returned invalid message arguments");
  }

  return {
    address: value.address,
    args,
  };
}

function isJsonOscArg(value: unknown): value is JsonOscArg {
  if (!isRecord(value) || typeof value.type !== "string") return false;

  switch (value.type) {
    case "int":
    case "int64":
    case "float":
      return typeof value.value === "number";
    case "str":
      return typeof value.value === "string";
    case "bool":
      return typeof value.value === "boolean";
    default:
      return false;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
