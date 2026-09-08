# Kitu Web Admin

Reusable browser administration surfaces for Kitu applications.

- `package/` publishes the Svelte 5 `@kitu/admin` package: the instance-scoped
  Admin client, context helpers, shell, controls, standard pages, CSS, and the
  OSC-IR WASM loader contract.
- `starter/` is a minimal independent SvelteKit consumer. It demonstrates
  app-owned routes, navigation, labels, endpoint configuration, base-path
  handling, and world appearance.

Application-specific routes, titles, domain types, and runtime hosts live in the
consuming repository. The maintained reference application is
[Nagitch/kitu-unity-demo-game](https://github.com/Nagitch/kitu-unity-demo-game).

## Package development

Use the repository-pinned Node and pnpm versions:

```sh
pnpm --dir tools/kitu-web-admin/package install --frozen-lockfile
pnpm --dir tools/kitu-web-admin/package run check
pnpm --dir tools/kitu-web-admin/package run test
pnpm --dir tools/kitu-web-admin/package run build
```

Build the common OSC-IR browser module with:

```sh
pnpm --dir tools/kitu-web-admin/package run wasm
```

The default output is
`tools/kitu-web-admin/package/public/kitu-osc-ir-wasm`. Set
`KITU_OSC_IR_WASM_CRATE` for another Kitu source checkout and
`KITU_ADMIN_WASM_OUT_DIR` for an absolute consumer public directory.

## Consuming the package

Import only public package entrypoints:

```ts
import { createAdminClient, setAdminClientContext } from '@kitu/admin/client'
import { AdminShell } from '@kitu/admin/ui'
import { OverviewPage, WorldPage } from '@kitu/admin/pages'
import '@kitu/admin/styles.css'
```

Create one client in the consuming root layout and inject it with
`setAdminClientContext`. Supply explicit HTTP, WebSocket, optional WebTransport,
KEP route, and loader base URLs from the application configuration. Pass the
router base path to `AdminShell` so links remain under deployments such as
`/demo/`.

The standard pages use these host contracts:

| Transport | Contract |
| --- | --- |
| `GET /app-actions` | Returns the application action catalog. |
| `POST /app-actions/{actionId}/run` | Accepts typed inputs and returns the action result. |
| `GET /shell/catalog` | Returns the Shell version, session identity, and catalog. |
| `POST /shell/line` | Executes one identified Shell request. |
| WebSocket | Receives `ServerEvent` JSON and sends fallback OSC JSON. |
| WebTransport (optional) | Sends the same OSC messages in KEP frames on the configured route. |

See [`package/README.md`](package/README.md) for the complete public contract.

## Starter

The starter generates its WASM assets before development or production builds:

```sh
pnpm --dir tools/kitu-web-admin/starter install --frozen-lockfile
pnpm --dir tools/kitu-web-admin/starter run check
pnpm --dir tools/kitu-web-admin/starter run build
```

It expects a compatible application host at the configured endpoints; the
framework repository does not start or own an application runtime.
