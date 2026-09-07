import { base } from "$app/paths";
import { createOscIrLoader } from "@kitu/admin/wasm";

/** Creates an Arena loader whose assets remain under the configured SvelteKit base path. */
export function createArenaOscIrLoader(origin = window.location.origin) {
  const baseUrl = new URL(`${base || ""}/`, origin);
  return createOscIrLoader({ baseUrl });
}
