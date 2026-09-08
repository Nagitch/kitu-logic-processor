# @kitu/admin

Reusable Svelte 5 components, standard Kitu Admin pages, and an instance-scoped
Admin transport client. Applications own their routes, navigation, labels,
endpoint configuration, and world appearance.

```ts
import { createAdminClient, setAdminClientContext } from '@kitu/admin/client'

const client = createAdminClient({
  apiUrl: 'http://localhost:8787',
  webSocketUrl: 'ws://localhost:8787/ws',
  webTransportUrl: 'https://localhost:9443',
  kepRoute: '/room/main'
})
setAdminClientContext(client) // in the consuming root layout
```

`AdminShell` receives app-owned `brand`, `subtitle`, and navigation sections.
Pass the consuming router's base path through `basePath`; internal navigation
then remains under deployments such as `/demo/`. Its optional `header` snippet
adds application controls beside the shared transport status without importing
package internals.

The consuming application owns the runtime/server and maps its deployment
configuration into `createAdminClient`. The common pages require these
contracts; their public payload types are exported from `@kitu/admin/client`.

| Transport | Contract |
| --- | --- |
| `GET /app-actions` | Returns the `AppActionCatalog`. |
| `POST /app-actions/{actionId}/run` | Accepts typed action inputs and returns `ActionRunResponse`. |
| `GET /shell/catalog` | Returns the Shell version, session identity, and command catalog. |
| `POST /shell/line` | Accepts one identified Shell request and returns its result. |
| WebSocket | Receives `ServerEvent` JSON and sends `ClientOscMessage` JSON when used for OSC fallback. |
| WebTransport (optional) | Sends the same OSC messages in KEP frames on the configured route. |

Import `@kitu/admin/styles.css` after Tailwind. The package stylesheet declares
its own Tailwind source so shared component classes are included from a local
file dependency or an installed package. Route files stay application-owned
and render exports from `@kitu/admin/pages`.

`pnpm run wasm` builds the generated browser module into
`public/kitu-osc-ir-wasm`. Set `KITU_OSC_IR_WASM_CRATE` when the Kitu checkout is
elsewhere, and `KITU_ADMIN_WASM_OUT_DIR` when copying directly into a consuming
app's public directory. Configure
`createOscIrLoader({ baseUrl, moduleUrl, wasmUrl })`, using the consuming
router's explicit base path, for apps hosted below a base path.
