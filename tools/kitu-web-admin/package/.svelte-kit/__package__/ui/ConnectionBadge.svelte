<script lang="ts">
  import type { AdminClient } from '../client/create-admin-client.js'
  import { getAdminClient } from '../client/context.js'
  let { client = getAdminClient() }: { client?: AdminClient } = $props()
  const tone = {
    idle: 'bg-muted text-muted-foreground',
    connecting: 'bg-secondary text-secondary-foreground',
    open: 'bg-accent text-accent-foreground',
    closed: 'bg-muted text-muted-foreground',
    error: 'bg-destructive text-destructive-foreground'
  }
</script>

<span class={`inline-flex h-8 items-center rounded-md px-3 text-xs font-semibold ${tone[$client.connectionState]}`}>
  {$client.connectionState}
</span>
