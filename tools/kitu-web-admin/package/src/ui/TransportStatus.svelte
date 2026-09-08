<script lang="ts">
  import type { AdminClient } from '../client/create-admin-client.js'
  import { getAdminClient } from '../client/context.js'
  let { client = getAdminClient() }: { client?: AdminClient } = $props()
  const wsTone = {
    idle: 'bg-muted text-muted-foreground',
    connecting: 'bg-secondary text-secondary-foreground',
    open: 'bg-accent text-accent-foreground',
    closed: 'bg-muted text-muted-foreground',
    error: 'bg-destructive text-destructive-foreground'
  }
  const wtTone = {
    disabled: 'bg-muted text-muted-foreground',
    unsupported: 'bg-muted text-muted-foreground',
    connecting: 'bg-secondary text-secondary-foreground',
    ready: 'bg-accent text-accent-foreground',
    closed: 'bg-muted text-muted-foreground',
    error: 'bg-destructive text-destructive-foreground'
  }
  const sendTone = {
    idle: 'bg-muted text-muted-foreground',
    pending: 'bg-secondary text-secondary-foreground',
    sent: 'bg-accent text-accent-foreground',
    applied: 'bg-accent text-accent-foreground',
    fallback: 'bg-secondary text-secondary-foreground',
    failed: 'bg-destructive text-destructive-foreground'
  }
  const labels = { http: 'HTTP', none: 'none', webtransport: 'wt', 'websocket-fallback': 'ws fallback', websocket: 'ws' }
</script>

<div class="flex flex-wrap justify-end gap-2">
  <span
    class={`inline-flex h-8 min-w-20 items-center justify-center rounded-md px-3 text-xs font-semibold ${wsTone[$client.connectionState]}`}
    title="WebSocket">
    WS {$client.connectionState}
  </span>
  <span
    class={`inline-flex h-8 min-w-24 items-center justify-center rounded-md px-3 text-xs font-semibold ${wtTone[$client.webTransportState]}`}
    title={$client.webTransportDetail ?? 'WebTransport'}>
    WT {$client.webTransportState}
  </span>
  <span
    class={`inline-flex h-8 min-w-28 items-center justify-center rounded-md px-3 text-xs font-semibold ${sendTone[$client.lastOscSendStatus.phase]}`}
    title={$client.lastOscSendStatus.detail ?? 'Last OSC send'}>
    OSC {labels[$client.lastOscSendStatus.path]}
  </span>
</div>
