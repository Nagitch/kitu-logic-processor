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
