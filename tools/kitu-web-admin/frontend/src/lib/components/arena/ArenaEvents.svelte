<script lang="ts">
  import {
    completedTick,
    describeEvent,
    eventEntity,
    unsigned,
    type InspectionSnapshot,
    type InspectionEvent,
  } from "$lib/arena-inspection";
  let {
    snapshot,
    onseek,
    onselect,
  }: {
    snapshot: InspectionSnapshot;
    onseek: (tick: string) => void;
    onselect: (key: string) => void;
  } = $props();
  let currentRun = $state(true),
    query = $state(""),
    selected = $state<string | null>(null);
  let filtered = $derived(
    snapshot.events.entries
      .filter(
        (event) =>
          (!currentRun || event.run === snapshot.run) &&
          `${event.address} ${describeEvent(event).summary}`
            .toLowerCase()
            .includes(query.toLowerCase()),
      )
      .toReversed(),
  );
  let detail = $derived(
    snapshot.events.entries.find((event) => event.sequence === selected),
  );
  const seekable = (event: InspectionEvent) =>
    snapshot.mode.active &&
    !snapshot.mode.seeking &&
    completedTick(event.tick) < unsigned(snapshot.mode.totalTicks);
  function payload(event: InspectionEvent) {
    return event.detail.kind === "message"
      ? event.detail.messageJson
      : JSON.stringify(event.detail, null, 2);
  }
</script>

<div class="flex flex-wrap items-center gap-3">
  <label class="flex items-center gap-2 text-sm"
    ><input type="checkbox" bind:checked={currentRun} />Current run only</label
  ><input
    aria-label="Filter event address, kind or entity"
    bind:value={query}
    placeholder="Address, kind or entity"
    class="min-w-0 flex-1 rounded border border-border px-2 py-1.5 text-sm"
  />
</div>
<p class="my-2 text-xs text-muted-foreground">
  Events observed since this snapshot context · {snapshot.events.entries
    .length}/{snapshot.events.capacity} retained · {snapshot.events.dropped} dropped
  · {snapshot.events.omittedPayloads} payloads omitted
</p>
<div class="max-h-64 overflow-auto rounded border border-border">
  {#each filtered as event (event.sequence)}<button
      type="button"
      onclick={() => (selected = event.sequence)}
      aria-pressed={selected === event.sequence}
      class={`block w-full border-b border-border px-3 py-2 text-left last:border-0 ${selected === event.sequence ? "bg-blue-50" : "hover:bg-muted"}`}
      ><span class="block break-all font-mono text-xs">{event.address}</span
      ><span class="mt-1 block text-xs">{describeEvent(event).summary}</span
      ><span class="mt-1 block text-xs text-muted-foreground"
        >Tick {event.tick} · run {event.run} · sequence {event.sequence} · {event.bundleIndex}:{event.messageIndex}{event
          .detail.kind === "oversize"
          ? " · payload omitted"
          : ""}</span
      ></button
    >{:else}<p class="p-3 text-sm text-muted-foreground">
      No matching observed events.
    </p>{/each}
</div>
{#if detail}{@const target = eventEntity(detail, snapshot)}
  <div class="mt-3">
    <div class="flex flex-wrap items-center justify-between gap-2">
      <h3 class="text-xs font-semibold">
        Event {detail.sequence} · revision {detail.revision}
      </h3>
      {#if seekable(detail)}<button
          class="rounded border border-border px-2 py-1 text-xs hover:bg-muted"
          onclick={() => onseek(detail.tick)}>Seek to tick {detail.tick}</button
        >{/if}
    </div>
    {#if target}<button
        class="mt-2 text-xs text-blue-700 underline"
        onclick={() => onselect(target.key)}
        >Select {target.kind} #{target.id}</button
      >{:else if describeEvent(detail).entity}<p
        class="mt-2 text-xs text-muted-foreground"
      >
        Referenced entity is absent from this run/snapshot.
      </p>{/if}
    <pre
      class="mt-2 max-h-56 overflow-auto whitespace-pre-wrap break-all rounded bg-slate-50 p-3 text-xs">{payload(
        detail,
      )}</pre>
  </div>{/if}
<p class="mt-2 text-xs text-muted-foreground">
  Newest first; bundle/message order is retained. Fast-forward does not
  reconstruct the event window. Removed entities remain historical event
  references.
</p>
