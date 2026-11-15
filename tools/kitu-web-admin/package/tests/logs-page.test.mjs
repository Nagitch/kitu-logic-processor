import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compile } from 'svelte/compiler'
import { render } from 'svelte/server'
import { createAdminClient, setAdminClientContext } from '../dist/client/index.js'

test('Debug Logs exposes the full multiline message in a disclosure', async () => {
  const hooks = registerHooks({
    load(url, context, nextLoad) {
      if (!url.endsWith('.svelte')) return nextLoad(url, context)
      const source = readFileSync(new URL(url), 'utf8')
      const { js } = compile(source, { filename: url, generate: 'server' })
      return { format: 'module', shortCircuit: true, source: js.code }
    }
  })
  try {
    const { default: LogsPage } = await import('../dist/pages/LogsPage.svelte')
    const client = createAdminClient({ apiUrl: 'https://example.test', webSocketUrl: 'wss://example.test/ws' })
    const message = `Job queued\n  ${'a'.repeat(240)}\nJob completed.`
    client.applyServerEvent({
      type: 'log',
      entry: { id: 1, tick: 42, level: 'info', message, oscAddress: '/jobs/status' }
    })
    const { body } = render(renderer => {
      setAdminClientContext(client)
      LogsPage(renderer)
    })

    const disclosure = body.match(/<details>\s*<summary\b[^>]*>[\s\S]*?<\/summary>\s*<div\b[^>]*>([\s\S]*?)<\/div>\s*<\/details>/)
    assert.ok(disclosure, 'expected an initially closed message disclosure')
    assert.equal(disclosure[1], message)
    assert.match(body, /<td\b[^>]*>\/jobs\/status<\/td>/)
  } finally {
    hooks.deregister()
  }
})
