<script lang="ts">
  import {
    entities,
    type InspectionSnapshot,
    type EntityKind,
  } from "$lib/arena-inspection";
  let {
    snapshot,
    selected,
    onselect,
  }: {
    snapshot: InspectionSnapshot;
    selected: string | null;
    onselect: (key: string) => void;
  } = $props();
  let kind = $state<EntityKind | "all">("all"),
    query = $state("");
  let actors = $derived(entities(snapshot));
  let filtered = $derived(
    actors.filter(
      (v) =>
        (kind === "all" || v.kind === kind) &&
        `${v.label} ${v.id}`.toLowerCase().includes(query.toLowerCase()),
    ),
  );
  let entity = $derived(actors.find((v) => v.key === selected));
</script>

<div class="flex gap-2">
  <select
    aria-label="Entity type"
    bind:value={kind}
    class="min-w-0 rounded border border-border p-2 text-sm"
    ><option value="all">All types</option
    >{#each ["player", "enemy", "projectile", "grenade", "effect"] as type}<option
        value={type}>{type}</option
      >{/each}</select
  >
  <input
    aria-label="Find entity by name or ID"
    placeholder="Name or ID"
    bind:value={query}
    class="min-w-0 flex-1 rounded border border-border px-2 text-sm"
  />
</div>
<p class="my-2 text-xs text-muted-foreground">
  {filtered.length} of {actors.length} entities · IDs are local to this run
</p>
<div class="max-h-56 overflow-y-auto rounded border border-border">
  {#each filtered as actor (actor.key)}
    <button
      type="button"
      class={`flex w-full items-center justify-between gap-2 border-b border-border px-3 py-2 text-left text-sm last:border-0 ${selected === actor.key ? "bg-blue-50 text-blue-800" : "hover:bg-muted"}`}
      aria-pressed={selected === actor.key}
      data-entity-key={actor.key}
      onclick={() => onselect(actor.key)}
    >
      <span>{actor.label}</span><span class="shrink-0 text-xs tabular-nums"
        >{actor.health === undefined
          ? `${actor.position.x.toFixed(2)}, ${actor.position.y.toFixed(2)}`
          : `${actor.health}/${actor.maxHealth} HP`}</span
      >
    </button>
  {:else}<p class="p-3 text-sm text-muted-foreground">
      No matching entities.
    </p>{/each}
</div>
{#if entity}
  <div class="mt-4">
    <h3 class="text-sm font-semibold">{entity.label}</h3>
    <p class="mt-1 text-xs text-muted-foreground">
      Position {entity.position.x.toFixed(4)}, {entity.position.y.toFixed(4)} · radius
      {entity.radius ?? "not defined"}
    </p>
    <pre
      class="mt-2 max-h-80 overflow-auto rounded bg-slate-50 p-3 text-xs">{JSON.stringify(
        entity.raw,
        null,
        2,
      )}</pre>
  </div>
{:else}<p class="mt-4 text-sm text-muted-foreground">
    Select an entity in the list or minimap. Selection clears when its run or
    snapshot context changes, or the entity disappears.
  </p>{/if}
