import { mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const here = dirname(fileURLToPath(import.meta.url));
const packageRoot = resolve(here, "..");
const crate = resolve(packageRoot, process.env.KITU_OSC_IR_WASM_CRATE ?? "../../../crates/kitu-osc-ir-wasm");
const outDir = resolve(packageRoot, process.env.KITU_ADMIN_WASM_OUT_DIR ?? "public/kitu-osc-ir-wasm");
mkdirSync(outDir, { recursive: true });
const result = spawnSync("wasm-pack", ["build", crate, "--target", "web", "--out-dir", outDir, "--out-name", "kitu_osc_ir_wasm"], { stdio: "inherit" });
if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
