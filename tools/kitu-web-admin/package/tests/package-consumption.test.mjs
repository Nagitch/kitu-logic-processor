import assert from 'node:assert/strict'
import test from 'node:test'
import { readFileSync } from 'node:fs'
import { compile } from 'svelte/compiler'
import { createAdminClient } from '../dist/client/index.js'
import { createOscIrLoader } from '../dist/wasm/index.js'
import { isAdminHrefActive, resolveAdminHref } from '../dist/ui/navigation.js'

test('public page and component entrypoints compile from the built package', () => {
  for (const file of [
    '../dist/pages/OverviewPage.svelte',
    '../dist/pages/LogsPage.svelte',
    '../dist/pages/ShellPage.svelte',
    '../dist/pages/AppActionsPage.svelte',
    '../dist/pages/WorldPage.svelte',
    '../dist/ui/AdminShell.svelte',
    '../dist/ui/Panel.svelte'
  ]) {
    const source = readFileSync(new URL(file, import.meta.url), 'utf8')
    const result = compile(source, { filename: file, generate: 'server' })
    assert.ok(result.js.code.length > 0, file)
  }
})

test('WASM loader resolves relative assets against a configured base URL', async () => {
  const imported = []
  const loader = createOscIrLoader({
    baseUrl: 'https://example.test/demo/',
    moduleUrl: 'assets/kitu.js',
    wasmUrl: 'assets/kitu.wasm',
    importModule: async url => {
      imported.push(url)
      return {
        default: async wasmUrl => imported.push(String(wasmUrl)),
        admin_world_spawn: () => ({ address: '/spawn', args: [] }),
        admin_world_move: () => ({ address: '/move', args: [] }),
        admin_world_reset: () => ({ address: '/reset', args: [] })
      }
    }
  })
  await loader.initialize()
  assert.deepEqual(imported, ['https://example.test/demo/assets/kitu.js', 'https://example.test/demo/assets/kitu.wasm'])
})

test('WASM loader captures one coherent module and binary URL pair', async () => {
  let finishImport
  const imported = []
  const options = {
    baseUrl: 'https://old.test/demo/',
    moduleUrl: 'module.js',
    wasmUrl: 'module.wasm',
    importModule: url => {
      imported.push(url)
      return new Promise(resolve => {
        finishImport = () =>
          resolve({
            default: async wasmUrl => imported.push(String(wasmUrl)),
            admin_world_spawn: () => ({ address: '/spawn', args: [] }),
            admin_world_move: () => ({ address: '/move', args: [] }),
            admin_world_reset: () => ({ address: '/reset', args: [] })
          })
      })
    }
  }
  const loader = createOscIrLoader(options)
  const pending = loader.initialize()
  options.baseUrl = 'https://new.test/'
  options.wasmUrl = 'other.wasm'
  finishImport()
  await pending
  assert.deepEqual(imported, ['https://old.test/demo/module.js', 'https://old.test/demo/module.wasm'])
})

test('Admin navigation stays inside a configured application base path', () => {
  assert.equal(resolveAdminHref('/'), '/')
  assert.equal(resolveAdminHref('/arena-inspector'), '/arena-inspector')
  assert.equal(resolveAdminHref('/', ''), '/')
  assert.equal(resolveAdminHref('/arena-inspector', ''), '/arena-inspector')
  assert.equal(resolveAdminHref('/', '/'), '/')
  assert.equal(resolveAdminHref('/arena-inspector', '/'), '/arena-inspector')
  assert.equal(resolveAdminHref('/', '/review'), '/review/')
  assert.equal(resolveAdminHref('/world', '/review/'), '/review/world')
  assert.equal(resolveAdminHref('https://example.test/admin', '/review'), 'https://example.test/admin')
  assert.equal(isAdminHrefActive('/review/world', '/world', '/review'), true)
  assert.equal(isAdminHrefActive('/review', '/', '/review'), true)
})

test('consumer can create a client without SvelteKit globals', () => {
  const client = createAdminClient({
    apiUrl: 'https://example.test/api',
    webSocketUrl: 'wss://example.test/ws',
    isBrowser: false
  })
  assert.equal(client.apiBaseUrl(), 'https://example.test/api')
})
