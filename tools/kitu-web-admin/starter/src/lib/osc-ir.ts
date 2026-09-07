import { base } from '$app/paths'
import { createOscIrLoader } from '@kitu/admin/wasm'

/** Creates a loader for the starter's generated WASM under its configured base path. */
export function createStarterOscIrLoader(origin = window.location.origin) {
  return createOscIrLoader({ baseUrl: new URL(`${base || ''}/`, origin) })
}
