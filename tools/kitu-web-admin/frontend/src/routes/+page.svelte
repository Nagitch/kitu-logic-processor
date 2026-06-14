<script lang="ts">
  import { Boxes, ScrollText } from "@lucide/svelte";
  import { debugLogs, lastError, worldSnapshot } from "$lib/admin-client";
  import Panel from "$lib/components/ui/Panel.svelte";
</script>

<svelte:head>
  <title>Kitu Admin</title>
</svelte:head>

<div class="grid gap-4">
  {#if $lastError}
    <div
      class="rounded-md border border-destructive/30 bg-destructive/10 px-4 py-3 text-sm text-destructive"
    >
      {$lastError}
    </div>
  {/if}

  <div class="grid grid-cols-3 gap-4 max-md:grid-cols-1">
    <Panel title="World Objects" eyebrow="Runtime">
      <div class="flex items-end justify-between gap-3">
        <p class="text-3xl font-semibold">{$worldSnapshot.objects.length}</p>
        <Boxes class="text-accent" size={26} />
      </div>
    </Panel>
    <Panel title="Runtime Tick" eyebrow="Scheduler">
      <p class="text-3xl font-semibold">{$worldSnapshot.tick}</p>
    </Panel>
    <Panel title="Log Entries" eyebrow="Debug">
      <div class="flex items-end justify-between gap-3">
        <p class="text-3xl font-semibold">{$debugLogs.length}</p>
        <ScrollText class="text-secondary" size={26} />
      </div>
    </Panel>
  </div>

  <div class="grid grid-cols-[1.4fr_0.8fr] gap-4 max-lg:grid-cols-1">
    <Panel title="Recent Objects">
      <div class="grid gap-2">
        {#each $worldSnapshot.objects.slice(0, 6) as object}
          <div
            class="flex items-center justify-between rounded-md border border-border px-3 py-2"
          >
            <div class="flex items-center gap-2">
              <span
                class="h-3 w-3 rounded-sm"
                style={`background: ${object.color}`}
              ></span>
              <span class="text-sm font-medium">{object.id}</span>
              <span class="text-xs text-muted-foreground">{object.kind}</span>
            </div>
            <span class="font-mono text-xs text-muted-foreground">
              {object.x.toFixed(1)}, {object.y.toFixed(1)}, {object.z.toFixed(
                1,
              )}
            </span>
          </div>
        {:else}
          <p class="text-sm text-muted-foreground">No objects</p>
        {/each}
      </div>
    </Panel>

    <Panel title="Recent Logs">
      <div class="grid gap-2">
        {#each $debugLogs.slice(0, 8) as entry}
          <div class="rounded-md border border-border px-3 py-2">
            <div class="flex items-center justify-between gap-2">
              <span class="text-xs font-semibold uppercase">{entry.level}</span>
              <span class="text-xs text-muted-foreground">#{entry.tick}</span>
            </div>
            <p class="mt-1 truncate text-sm">{entry.message}</p>
          </div>
        {:else}
          <p class="text-sm text-muted-foreground">No logs</p>
        {/each}
      </div>
    </Panel>
  </div>
</div>
