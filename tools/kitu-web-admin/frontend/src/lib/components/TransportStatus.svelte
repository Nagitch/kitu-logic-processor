<script lang="ts">
  import {
    connectionState,
    lastOscSendStatus,
    webTransportDetail,
    webTransportState,
  } from "$lib/admin-client";

  const websocketTone = {
    idle: "bg-muted text-muted-foreground",
    connecting: "bg-secondary text-secondary-foreground",
    open: "bg-accent text-accent-foreground",
    closed: "bg-muted text-muted-foreground",
    error: "bg-destructive text-destructive-foreground",
  };

  const webTransportTone = {
    disabled: "bg-muted text-muted-foreground",
    unsupported: "bg-muted text-muted-foreground",
    connecting: "bg-secondary text-secondary-foreground",
    ready: "bg-accent text-accent-foreground",
    closed: "bg-muted text-muted-foreground",
    error: "bg-destructive text-destructive-foreground",
  };

  const sendTone = {
    idle: "bg-muted text-muted-foreground",
    pending: "bg-secondary text-secondary-foreground",
    sent: "bg-accent text-accent-foreground",
    fallback: "bg-secondary text-secondary-foreground",
    failed: "bg-destructive text-destructive-foreground",
  };

  const sendPathLabel = {
    none: "none",
    webtransport: "wt",
    "websocket-fallback": "ws fallback",
    websocket: "ws",
    "app-action": "action",
  };
</script>

<div class="flex flex-wrap justify-end gap-2">
  <span
    class={`inline-flex h-8 min-w-20 items-center justify-center rounded-md px-3 text-xs font-semibold ${websocketTone[$connectionState]}`}
    title="WebSocket"
  >
    WS {$connectionState}
  </span>
  <span
    class={`inline-flex h-8 min-w-24 items-center justify-center rounded-md px-3 text-xs font-semibold ${webTransportTone[$webTransportState]}`}
    title={$webTransportDetail ?? "WebTransport"}
  >
    WT {$webTransportState}
  </span>
  <span
    class={`inline-flex h-8 min-w-28 items-center justify-center rounded-md px-3 text-xs font-semibold ${sendTone[$lastOscSendStatus.phase]}`}
    title={$lastOscSendStatus.detail ?? "Last OSC send"}
  >
    OSC {sendPathLabel[$lastOscSendStatus.path]}
  </span>
</div>
