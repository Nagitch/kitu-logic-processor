<script lang="ts">
  import { onMount } from "svelte";
  import { Check, FileCheck, RefreshCw } from "@lucide/svelte";
  import { Button, Panel } from "@kitu/admin/ui";
  import {
    inspectTimeline,
    validateTimeline,
    stageTimeline,
    type TimelineStatus,
  } from "$lib/arena-timeline";

  let status: TimelineStatus | null = null;
  let busy = false;
  let error = "";
  let notice = "";
  let selected: "candidate" | "pending" | "active" = "candidate";
  let requestId = 0;
  let disposed = false;

  $: version =
    selected === "active"
      ? status?.runtime.active
      : selected === "pending"
        ? status?.runtime.pending
        : status?.candidate;
  $: alreadyStaged =
    !!status?.candidate &&
    status.candidate.hash === status.runtime.pending.hash;

  const clipFiles = {
    "boss-telegraph": "boss-telegraph.tsq",
    "floor-transition": "floor-transition.tsq",
  };
  const clipNames = {
    "boss-telegraph": "Boss warning",
    "floor-transition": "Floor transition",
  };
  $: presentation = status?.runtime.presentation;
  $: cues = [
    ...(presentation?.bosses ?? []).map((cue) => ({
      ...cue,
      title: `Boss warning · Enemy ${cue.entityId}`,
      values: [
        { label: "Floor", value: cue.floor },
        { label: "Radius", value: cue.radius },
        { label: "Intensity", value: cue.intensity },
      ],
    })),
    ...(presentation?.floor
      ? [
          {
            ...presentation.floor,
            title: `Floor transition · ${presentation.floor.fromFloor} → ${presentation.floor.toFloor}`,
            values: [{ label: "Opacity", value: presentation.floor.opacity }],
          },
        ]
      : []),
  ];

  async function refresh() {
    if (busy) return;
    const id = ++requestId;
    try {
      const next = await inspectTimeline();
      if (!disposed && id === requestId) {
        status = next;
        error = "";
      }
    } catch (cause) {
      if (!disposed && id === requestId) error = String(cause);
    }
  }

  async function validate() {
    if (busy) return;
    busy = true;
    const id = ++requestId;
    notice = "";
    error = "";
    try {
      const next = await validateTimeline();
      if (!disposed && id === requestId) {
        status = next;
        selected = "candidate";
        if (next.candidate) notice = "Validated. Review the clips below.";
      }
    } catch (cause) {
      if (!disposed && id === requestId) error = String(cause);
    } finally {
      if (!disposed && id === requestId) busy = false;
    }
  }

  async function apply() {
    if (busy || status?.readOnly || !status?.candidate || alreadyStaged) return;
    const candidate = status.candidate;
    busy = true;
    const id = ++requestId;
    notice = "";
    error = "";
    try {
      const result = await stageTimeline(candidate);
      if (disposed || id !== requestId) return;
      notice = `Queued for the next run (command ${result.sequence}). The current run keeps its saved timelines.`;
      const next = await inspectTimeline();
      if (!disposed && id === requestId) status = next;
    } catch (cause) {
      if (!disposed && id === requestId) error = String(cause);
    } finally {
      if (!disposed && id === requestId) busy = false;
    }
  }

  onMount(() => {
    refresh();
    const timer = setInterval(() => {
      if (!busy && document.visibilityState === "visible") refresh();
    }, 1500);
    return () => {
      disposed = true;
      ++requestId;
      clearInterval(timer);
    };
  });
</script>

<svelte:head><title>Story Sequencing - Kitu Admin</title></svelte:head>

<div class="grid gap-4">
  <Panel title="Endless Arena timelines" eyebrow="Story sequencing">
    <div class="grid gap-4 text-sm">
      <p class="text-muted-foreground">
        Edit and save the TSQ1 clips, reload and validate them here, then apply
        the reviewed version to the next run. Timelines control boss warnings
        and floor transition effects.
      </p>
      {#if status?.readOnly}
        <p class="text-amber-700">
          Replay inspection is read-only. Return to the live run before applying
          timelines.
        </p>
      {/if}
      {#if status}
        <div>
          <p class="text-muted-foreground">Configured source directory</p>
          <p class="break-all font-mono text-xs">
            {status.path ?? "Bundled defaults"}
          </p>
          <p class="mt-1 text-muted-foreground">
            {#if status.path === null}
              Configure a timeline directory on the host to edit
            {:else}
              This directory contains
            {/if}
            <code>boss-telegraph.tsq</code> and
            <code>floor-transition.tsq</code>.
          </p>
        </div>
      {/if}
      <div class="flex flex-wrap gap-2">
        <Button onclick={validate} disabled={busy}
          ><FileCheck size={16} />
          {busy ? "Working…" : "Reload and validate"}</Button
        >
        <Button
          variant="secondary"
          onclick={apply}
          disabled={busy ||
            status?.readOnly ||
            !status?.candidate ||
            alreadyStaged}><Check size={16} /> Apply to next run</Button
        >
        <Button variant="ghost" onclick={refresh} disabled={busy}
          ><RefreshCw size={16} /> Refresh</Button
        >
      </div>
      {#if notice}<p role="status">{notice}</p>{/if}
      {#if error}<p role="alert" class="text-destructive">{error}</p>{/if}
      {#if status?.diagnostics.length}
        <div role="alert" class="rounded-md border border-destructive p-3">
          <p class="font-semibold">This edit cannot be applied.</p>
          <p>The current and next run keep their last valid timelines.</p>
          {#each status.diagnostics as diagnostic}
            <p class="mt-2 whitespace-pre-wrap break-words font-mono text-xs">
              {diagnostic}
            </p>
          {/each}
        </div>
      {/if}
      {#if status}
        <dl class="grid gap-4 rounded-md border border-border p-3">
          {#each [{ label: `Current run · ${status.runtime.run}`, hash: status.runtime.active?.hash ?? "No run started" }, { label: "Next run", hash: status.runtime.pending.hash }, { label: "Validated edit", hash: status.candidate?.hash ?? "No validated edit" }] as item}
            <div>
              <dt class="text-muted-foreground">{item.label}</dt>
              <dd class="break-all font-mono text-xs">{item.hash}</dd>
            </div>
          {/each}
        </dl>
      {:else if !error}
        <p role="status" class="text-muted-foreground">
          Loading timeline status…
        </p>
      {/if}
    </div>
  </Panel>

  <Panel title="Timeline clips" eyebrow="Review">
    <div class="grid gap-4 text-sm">
      <label class="flex flex-wrap items-center gap-3 font-medium">
        Version
        <select
          bind:value={selected}
          class="rounded-md border border-border bg-background px-3 py-2"
        >
          <option value="candidate">Validated edit</option>
          <option value="pending">Next run</option>
          <option value="active">Current run</option>
        </select>
      </label>
      {#if version}
        <div>
          <p class="text-muted-foreground">Timeline version</p>
          <p class="break-all font-mono text-xs">{version.hash}</p>
          <p class="mt-2 text-muted-foreground">
            {version.tickRate} ticks per second · Contract {version.contractVersion}
          </p>
        </div>
        <div class="overflow-x-auto">
          <table class="w-full text-left text-xs">
            <caption class="mb-2 text-left text-muted-foreground"
              >Clips stored in the selected version</caption
            >
            <thead class="border-b border-border"
              ><tr>
                <th scope="col" class="p-2 font-medium">Clip</th>
                <th scope="col" class="p-2 font-medium">TSQ1 file</th>
                <th scope="col" class="p-2 font-medium">Source digest</th>
                <th scope="col" class="p-2 font-medium">Bytes</th>
              </tr></thead
            >
            <tbody
              >{#each version.clips as clip (clip.id)}
                <tr class="border-b border-border">
                  <th scope="row" class="p-2 font-medium"
                    >{clipNames[clip.id]}</th
                  >
                  <td class="whitespace-nowrap p-2 font-mono"
                    >{clipFiles[clip.id]}</td
                  >
                  <td class="min-w-40 break-all p-2 font-mono"
                    >{clip.sourceSha256}</td
                  >
                  <td class="p-2 tabular-nums">{clip.bytes.length}</td>
                </tr>
              {/each}</tbody
            >
          </table>
        </div>
      {:else}
        <p class="text-muted-foreground">
          {selected === "active"
            ? "No run has started yet."
            : "No validated edit. Reload and validate the clips or select another version."}
        </p>
      {/if}
    </div>
  </Panel>

  <Panel title="Playing cues" eyebrow="Observed run">
    <div class="grid gap-4 text-sm">
      <p class="text-muted-foreground">
        Positions and values follow the Runtime clock. Use
        <a class="underline underline-offset-4" href="/arena-replay"
          >Arena Replay</a
        >
        to stop, step or seek a saved run.
      </p>
      {#if presentation}
        {#if status?.runtime.active}
          <div>
            <p class="text-muted-foreground">Run timeline version</p>
            <p class="break-all font-mono text-xs">
              {status.runtime.active.hash}
            </p>
          </div>
        {/if}
        <dl class="flex flex-wrap gap-x-8 gap-y-3">
          {#each [{ label: "Run", value: presentation.run }, { label: "Runtime tick", value: presentation.tick }, { label: "Game updates", value: presentation.simulationStep }] as item}
            <div>
              <dt class="text-muted-foreground">{item.label}</dt>
              <dd class="font-mono">{item.value}</dd>
            </div>
          {/each}
        </dl>
        <div class="grid gap-4">
          {#each cues as cue (cue.id)}
            <article class="grid gap-3 rounded-md border border-border p-3">
              <h3 class="font-semibold">{cue.title}</h3>
              <p class="break-all font-mono text-xs">{clipFiles[cue.clipId]}</p>
              <dl class="flex flex-wrap gap-x-8 gap-y-2">
                <div>
                  <dt class="text-muted-foreground">Position</dt>
                  <dd class="font-mono">{cue.offsetTick} ticks</dd>
                </div>
                <div>
                  <dt class="text-muted-foreground">Started at tick</dt>
                  <dd class="font-mono">{cue.startedTick}</dd>
                </div>
                {#each cue.values as item}
                  <div>
                    <dt class="text-muted-foreground">{item.label}</dt>
                    <dd class="font-mono">{item.value}</dd>
                  </div>
                {/each}
              </dl>
              <label class="grid gap-1 text-xs text-muted-foreground">
                Events emitted · {cue.nextEventIndex} / {cue.eventCount}
                <progress
                  class="h-2 w-full accent-primary"
                  value={cue.nextEventIndex}
                  max={Math.max(1, cue.eventCount)}
                ></progress>
              </label>
            </article>
          {:else}
            <p class="text-muted-foreground">
              No boss warning or floor transition is active.
            </p>
          {/each}
        </div>
      {:else}
        <p class="text-muted-foreground">Waiting for a Runtime snapshot.</p>
      {/if}
    </div>
  </Panel>
</div>
