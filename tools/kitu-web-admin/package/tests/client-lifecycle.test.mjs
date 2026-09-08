import assert from 'node:assert/strict'
import test from 'node:test'
import { get } from 'svelte/store'
import { createAdminClient } from '../dist/client/index.js'

class FakeSocket extends EventTarget {
  readyState = 0
  sent = []
  send(value) {
    this.sent.push(value)
  }
  close() {
    this.readyState = 3
    this.dispatchEvent(new Event('close'))
  }
  open() {
    this.readyState = 1
    this.dispatchEvent(new Event('open'))
  }
  message(value) {
    this.dispatchEvent(new MessageEvent('message', { data: JSON.stringify(value) }))
  }
}

test('server errors remain in log history after a successful operation clears lastError', async () => {
  const socket = new FakeSocket()
  const client = createAdminClient({
    apiUrl: 'http://service.test',
    webSocketUrl: 'ws://service.test/ws',
    isBrowser: true,
    createWebSocket: () => socket,
    fetch: async () => new Response(JSON.stringify({ actions: [] }))
  })
  client.start()
  socket.open()
  socket.message({ type: 'state', snapshot: { tick: 42, objects: [] } })
  socket.message({ type: 'error', message: 'Rejected application command' })
  assert.equal(get(client.lastError), 'Rejected application command')
  assert.equal(client.sendOsc({ address: '/admin/world/reset', args: [] }), true)
  assert.equal(get(client.lastError), null)
  const entries = get(client.debugLogs)
  assert.equal(entries.length, 1)
  assert.equal(entries[0].level, 'error')
  assert.equal(entries[0].message, 'Rejected application command')
  assert.equal(entries[0].tick, 42)
  assert.equal(entries[0].oscAddress, null)
  client.stop()
})

test('server error history shares the bounded newest-first log buffer', () => {
  const client = createAdminClient({ apiUrl: '', webSocketUrl: '', isBrowser: false })
  client.applyServerEvent({ type: 'log', entry: { id: 1, level: 'info', message: 'old', tick: 0 } })
  for (let index = 0; index < 501; index += 1)
    client.applyServerEvent({ type: 'error', message: `error ${index}` })
  const entries = get(client.debugLogs)
  assert.equal(entries.length, 500)
  assert.equal(entries[0].message, 'error 500')
  assert.equal(entries.at(-1).message, 'error 1')
  assert.ok(entries.every(entry => entry.level === 'error'))
})

test('each client owns independent socket and store lifecycle', async () => {
  const sockets = []
  const options = {
    apiUrl: 'http://service.test',
    webSocketUrl: 'ws://service.test/ws',
    isBrowser: true,
    createWebSocket: () => {
      const socket = new FakeSocket()
      sockets.push(socket)
      return socket
    },
    fetch: async () => new Response(JSON.stringify({ actions: [] }))
  }
  const first = createAdminClient(options)
  const second = createAdminClient(options)
  first.start()
  second.start()
  assert.equal(sockets.length, 2)
  sockets[0].open()
  sockets[1].open()
  sockets[0].message({
    type: 'state',
    snapshot: { tick: 7, objects: [{ id: 'one', kind: 'marker', x: 0, y: 0, z: 0, color: '#fff' }] }
  })
  assert.equal(get(first.worldSnapshot).tick, 7)
  assert.equal(get(second.worldSnapshot).tick, 0)
  first.stop()
  assert.equal(get(first.connectionState), 'idle')
  assert.equal(get(second.connectionState), 'open')
  second.stop()
})

test('HTTP app action updates only its client snapshot', async () => {
  const client = createAdminClient({
    apiUrl: 'http://service.test',
    webSocketUrl: 'ws://service.test/ws',
    isBrowser: false,
    fetch: async () =>
      new Response(
        JSON.stringify({
          actionId: 'reset-world',
          osc: { address: '/admin/world/reset', args: [] },
          snapshot: { tick: 9, objects: [] }
        })
      )
  })
  assert.equal(await client.resetWorld(), true)
  assert.equal(get(client.worldSnapshot).tick, 9)
  assert.equal(get(client.lastOscSendStatus).phase, 'applied')
})

test('client captures options and ignores a response from an earlier lifecycle', async () => {
  let resolveAction
  const action = new Promise(resolve => {
    resolveAction = resolve
  })
  const sockets = []
  const options = {
    apiUrl: 'http://first.test',
    webSocketUrl: 'ws://first.test/ws',
    isBrowser: true,
    createWebSocket: () => {
      const socket = new FakeSocket()
      sockets.push(socket)
      return socket
    },
    fetch: async url => (url.endsWith('/run') ? action : new Response(JSON.stringify({ actions: [] })))
  }
  const client = createAdminClient(options)
  options.apiUrl = 'http://mutated.test'
  assert.equal(client.apiBaseUrl(), 'http://first.test')
  client.start()
  sockets.at(-1).open()
  const pending = client.resetWorld()
  client.stop()
  client.start()
  sockets.at(-1).open()
  client.applyServerEvent({ type: 'state', snapshot: { tick: 99, objects: [] } })
  resolveAction(
    new Response(
      JSON.stringify({
        actionId: 'reset-world',
        osc: { address: '/reset', args: [] },
        snapshot: { tick: 1, objects: [] }
      })
    )
  )
  assert.equal(await pending, false)
  assert.equal(get(client.worldSnapshot).tick, 99)
  client.stop()
})

test('stop cancels reconnect and old socket events cannot alter the next lifecycle', async () => {
  const sockets = []
  const client = createAdminClient({
    apiUrl: 'http://service.test',
    webSocketUrl: 'ws://service.test/ws',
    reconnectDelayMs: 5,
    isBrowser: true,
    createWebSocket: () => {
      const socket = new FakeSocket()
      sockets.push(socket)
      return socket
    },
    fetch: async () => new Response(JSON.stringify({ actions: [] }))
  })
  client.start()
  const old = sockets[0]
  old.open()
  old.close()
  client.stop()
  await new Promise(resolve => setTimeout(resolve, 15))
  assert.equal(sockets.length, 1)
  client.start()
  const current = sockets[1]
  current.open()
  old.dispatchEvent(new Event('error'))
  assert.equal(get(client.connectionState), 'open')
  client.stop()
})

test('a reconnect generation invalidates an in-flight response from the closed socket', async () => {
  let resolveAction
  const action = new Promise(resolve => {
    resolveAction = resolve
  })
  const sockets = []
  const client = createAdminClient({
    apiUrl: 'http://service.test',
    webSocketUrl: 'ws://service.test/ws',
    reconnectDelayMs: 1,
    isBrowser: true,
    createWebSocket: () => {
      const socket = new FakeSocket()
      sockets.push(socket)
      return socket
    },
    fetch: async url => (url.endsWith('/run') ? action : new Response(JSON.stringify({ actions: [] })))
  })
  client.start()
  sockets[0].open()
  const pending = client.resetWorld()
  sockets[0].close()
  await new Promise(resolve => setTimeout(resolve, 5))
  sockets[1].open()
  client.applyServerEvent({ type: 'state', snapshot: { tick: 42, objects: [] } })
  resolveAction(
    new Response(
      JSON.stringify({
        actionId: 'reset-world',
        osc: { address: '/reset', args: [] },
        snapshot: { tick: 1, objects: [] }
      })
    )
  )
  assert.equal(await pending, false)
  assert.equal(get(client.worldSnapshot).tick, 42)
  client.stop()
})
