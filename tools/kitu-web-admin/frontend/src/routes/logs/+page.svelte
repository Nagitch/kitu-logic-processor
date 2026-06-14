<script lang="ts">
  import { Search } from "@lucide/svelte";
  import { debugLogs } from "$lib/admin-client";
  import Input from "$lib/components/ui/Input.svelte";
  import Panel from "$lib/components/ui/Panel.svelte";

  let query = "";
  let level = "all";

  $: filtered = $debugLogs.filter((entry) => {
    const matchesLevel = level === "all" || entry.level === level;
    const text = `${entry.message} ${entry.oscAddress ?? ""}`.toLowerCase();
    return matchesLevel && text.includes(query.toLowerCase());
  });

  const levelClass = {
    info: "bg-accent/10 text-accent",
    warn: "bg-secondary/20 text-secondary-foreground",
    error: "bg-destructive/10 text-destructive",
  };
</script>

<svelte:head>
  <title>Logs - Kitu Admin</title>
</svelte:head>

<Panel title="Debug Logs" eyebrow="Event Stream">
  <div class="mb-4 grid grid-cols-[1fr_160px] gap-3 max-sm:grid-cols-1">
    <label class="relative">
      <Search
        class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-muted-foreground"
        size={16}
      />
      <Input bind:value={query} placeholder="Filter" class="pl-9" />
    </label>
    <select
      bind:value={level}
      class="h-10 rounded-md border border-input bg-background px-3 text-sm outline-none focus:border-ring focus:ring-2 focus:ring-ring/20"
    >
      <option value="all">All levels</option>
      <option value="info">Info</option>
      <option value="warn">Warn</option>
      <option value="error">Error</option>
    </select>
  </div>

  <div class="overflow-hidden rounded-md border border-border">
    <table class="w-full table-fixed border-collapse text-sm">
      <thead class="bg-muted text-xs uppercase text-muted-foreground">
        <tr>
          <th class="w-20 px-3 py-2 text-left font-semibold">Tick</th>
          <th class="w-24 px-3 py-2 text-left font-semibold">Level</th>
          <th class="px-3 py-2 text-left font-semibold">Message</th>
          <th class="w-56 px-3 py-2 text-left font-semibold max-md:hidden"
            >OSC</th
          >
        </tr>
      </thead>
      <tbody>
        {#each filtered as entry}
          <tr class="border-t border-border bg-white">
            <td class="px-3 py-2 font-mono text-xs text-muted-foreground"
              >{entry.tick}</td
            >
            <td class="px-3 py-2">
              <span
                class={`inline-flex rounded-md px-2 py-1 text-xs font-semibold ${levelClass[entry.level]}`}
              >
                {entry.level}
              </span>
            </td>
            <td class="truncate px-3 py-2">{entry.message}</td>
            <td
              class="truncate px-3 py-2 font-mono text-xs text-muted-foreground max-md:hidden"
            >
              {entry.oscAddress ?? "-"}
            </td>
          </tr>
        {:else}
          <tr class="border-t border-border bg-white">
            <td colspan="4" class="px-3 py-8 text-center text-muted-foreground"
              >No logs</td
            >
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</Panel>
