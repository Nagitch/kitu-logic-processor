<script lang="ts">
  import { onMount } from "svelte";
  import { apiBaseUrl } from "$lib/admin-client";
  import Panel from "$lib/components/ui/Panel.svelte";
  import ArenaMinimap from "$lib/components/arena/ArenaMinimap.svelte";
  import ArenaEntities from "$lib/components/arena/ArenaEntities.svelte";
  import ArenaEvents from "$lib/components/arena/ArenaEvents.svelte";
  import ArenaTiming from "$lib/components/arena/ArenaTiming.svelte";
  import {
    createInspectionPoller,
    preserveSelection,
    contextKey,
    entities,
    replayAction,
    replayIntent,
    confirmReplay,
    completedTick,
    unsigned,
    type InspectionView,
    type ReplayAction,
    type ReplayIntent,
  } from "$lib/arena-inspection";
  let view = $state<InspectionView>({
    snapshot: null,
    status: "idle",
    error: null,
    updatedAt: null,
  });
  let selected = $state<string | null>(null),
    seekTarget = $state("-1"),
    actionMessage = $state<string | null>(null),
    actionError = $state(false),
    busy = $state(false);
  let snapshot = $derived(view.snapshot);
  let endpoint = $state("");
  let poller: ReturnType<typeof createInspectionPoller> | null = null;
  let commandAbort: AbortController | null = null,
    commandDeadline: ReturnType<typeof setTimeout> | null = null;
  let pending: ReplayIntent | null = null,
    acknowledged = false,
    mounted = false;
  const phases = [
    "Opening",
    "Preparation",
    "Transition",
    "Combat",
    "Cleared",
    "Results",
  ];
  function finish(message: string, failed = false) {
    if (commandDeadline !== null) clearTimeout(commandDeadline);
    commandDeadline = null;
    pending = null;
    busy = false;
    acknowledged = false;
    commandAbort?.abort();
    commandAbort = null;
    actionMessage = message;
    actionError = failed;
  }
  function confirm(next: NonNullable<InspectionView["snapshot"]>) {
    if (!pending || !acknowledged) return;
    try {
      if (confirmReplay(pending, next) === "confirmed")
        finish(`${pending.action} confirmed at tick ${next.mode.tick}.`);
    } catch (error) {
      finish(error instanceof Error ? error.message : String(error), true);
    }
  }
  async function act(action: ReplayAction, target = seekTarget) {
    if (!snapshot || busy || view.status !== "current") return;
    const baseline = snapshot;
    try {
      pending = replayIntent(baseline, action, target);
    } catch (error) {
      actionMessage = error instanceof Error ? error.message : String(error);
      actionError = true;
      return;
    }
    busy = true;
    acknowledged = false;
    actionError = false;
    actionMessage = `${action}: waiting for the Arena owner clock…`;
    const controller = new AbortController();
    commandAbort = controller;
    commandDeadline = setTimeout(
      () =>
        finish(
          "Command confirmation timed out. Refresh the observation to inspect the owner-applied result.",
          true,
        ),
      30000,
    );
    try {
      await replayAction(
        endpoint,
        action,
        target,
        baseline.mode.totalTicks,
        controller.signal,
      );
      if (!mounted || commandAbort !== controller) return;
      acknowledged = true;
      // Always fetch after acknowledgement, even if a prior poll saw the effect.
      await poller?.refresh();
      if (snapshot && view.status === "current") confirm(snapshot);
    } catch (error) {
      if (mounted && commandAbort === controller)
        finish(error instanceof Error ? error.message : String(error), true);
    }
  }
  onMount(() => {
    mounted = true;
    endpoint = apiBaseUrl();
    poller = createInspectionPoller({
      endpoint,
      changed(next) {
        const previous = view.snapshot;
        view = next;
        if (next.snapshot) {
          selected = preserveSelection(selected, next.snapshot);
          if (!previous || contextKey(previous) !== contextKey(next.snapshot)) {
            seekTarget = next.snapshot.state.tick;
            if (!busy) actionMessage = null;
          }
          if (next.status === "current") confirm(next.snapshot);
        }
      },
    });
    const visibility = () => {
      poller?.setVisible(!document.hidden);
      if (document.hidden && busy)
        finish(
          "Observation paused while hidden. Any admitted command may still complete; inspect the refreshed state on return.",
          true,
        );
    };
    document.addEventListener("visibilitychange", visibility);
    visibility();
    poller.start();
    return () => {
      mounted = false;
      poller?.stop();
      poller = null;
      commandAbort?.abort();
      if (commandDeadline !== null) clearTimeout(commandDeadline);
      document.removeEventListener("visibilitychange", visibility);
    };
  });
</script>

<svelte:head
  ><title>Arena Inspector · Kitu Admin</title><meta
    name="description"
    content="Inspect the authoritative Endless Arena run, replay, entities, events and host timings."
  /></svelte:head
>
<div
  class="grid min-w-0 gap-4"
  data-testid="arena-inspector"
  data-session-id={snapshot?.sessionId}
  data-run={snapshot?.run}
  data-tick={snapshot?.state.tick}
  data-epoch={snapshot?.epoch}
  data-revision={snapshot?.revision}
  data-attempt={snapshot?.attempt}
>
  <div class="flex flex-wrap items-start justify-between gap-3">
    <div>
      <h1 class="text-xl font-semibold">Arena Inspector</h1>
      <p class="mt-1 text-sm text-muted-foreground">
        One authoritative observation for the run, its entities and
        presentation.
      </p>
      <p class="mt-1 break-all font-mono text-xs text-muted-foreground">
        {endpoint || "Connecting…"}
      </p>
    </div>
    <div class="flex items-center gap-2">
      <span
        class={`rounded-full px-3 py-1 text-xs font-semibold ${view.status === "current" ? "bg-emerald-50 text-emerald-800" : "bg-amber-50 text-amber-800"}`}
        data-testid="inspection-status"
        >{view.status === "current" ? "Current · 5 Hz" : view.status}</span
      ><button
        class="rounded border border-border bg-white px-3 py-1.5 text-sm disabled:opacity-50"
        disabled={view.status === "hidden"}
        onclick={() => poller?.refresh()}>Refresh</button
      >
    </div>
  </div>
  {#if view.error}<p
      role="alert"
      class="rounded border border-amber-200 bg-amber-50 p-3 text-sm text-amber-900"
    >
      {view.error}{snapshot ? " · Showing the last valid observation." : ""}
    </p>{/if}
  {#if view.status === "hidden"}<p class="text-sm text-muted-foreground">
      Inspection is paused while this tab is hidden.
    </p>{/if}
  {#if snapshot}
    <section
      aria-label="Arena context"
      class="rounded-lg border border-border bg-white p-4"
    >
      <div class="flex flex-wrap items-center gap-x-6 gap-y-3">
        <div>
          <p class="text-xs uppercase text-muted-foreground">Mode</p>
          <p class="font-semibold">
            {snapshot.mode.active
              ? snapshot.mode.playing
                ? "Replay · playing"
                : "Replay · paused"
              : "Live"}{snapshot.mode.seeking ? " · preparing snapshot" : ""}
          </p>
        </div>
        {#each [["Run", snapshot.run], ["Tick", snapshot.state.tick], ["Simulation steps", snapshot.state.simulationSteps], ["Floor", snapshot.state.floor], ["State", phases[snapshot.state.phase]], ["Overlay", snapshot.state.overlay]] as [label, value]}<div
          >
            <p class="text-xs uppercase text-muted-foreground">{label}</p>
            <p class="font-mono text-sm font-semibold">{value}</p>
          </div>{/each}
      </div>
      <p class="mt-3 break-all font-mono text-xs text-muted-foreground">
        Session {snapshot.sessionId} · epoch {snapshot.epoch} · revision {snapshot.revision}
        · attempt {snapshot.attempt}
      </p>
      {#if snapshot.mode.recordingId}<p
          class="mt-1 break-all font-mono text-xs text-muted-foreground"
        >
          Recording {snapshot.mode.recordingId} · {snapshot.mode.totalTicks} ticks
        </p>{/if}
      <details class="mt-3">
        <summary class="cursor-pointer text-xs font-medium"
          >Execution and content versions</summary
        >
        <div class="mt-2 grid gap-2 break-all font-mono text-xs">
          <p>
            {snapshot.execution.package} · {snapshot.execution.target}<br
            />Source {snapshot.execution.sourceHash}
          </p>
          {#each Object.entries(snapshot.versions) as [name, version]}<p>
              <span class="font-semibold">{name}</span><br />Active {version.activeHash ??
                "no run started"}<br />Next run {version.pendingHash}
            </p>{/each}
        </div>
      </details>
    </section>
    {#if snapshot.diagnostics.hostError || snapshot.diagnostics.recordingError || snapshot.diagnostics.scriptFault || snapshot.mode.error}<div
        role="alert"
        class="grid gap-2 rounded border border-red-200 bg-red-50 p-3 text-sm text-red-900"
      >
        {#if snapshot.diagnostics.hostError}<p>
            Host: {snapshot.diagnostics.hostError}
          </p>{/if}{#if snapshot.diagnostics.recordingError}<p>
            Recording: {snapshot.diagnostics.recordingError}
          </p>{/if}{#if snapshot.mode.error}<p>
            Replay: {snapshot.mode.error}
          </p>{/if}{#if snapshot.diagnostics.scriptFault}<p>
            Script failed at tick {snapshot.diagnostics.scriptFault.tick}, enemy
            #{snapshot.diagnostics.scriptFault.enemyId}: {snapshot.diagnostics
              .scriptFault.diagnostic.message}
          </p>{/if}
      </div>{/if}
    <Panel title="Replay controls" eyebrow="Owner-clock operations">
      <div class="flex flex-wrap items-center gap-2">
        {#each ["play", "pause", "step", "stop", "live"] as action}<button
            class="rounded border border-border px-3 py-1.5 text-sm capitalize hover:bg-muted disabled:opacity-40"
            disabled={busy ||
              view.status !== "current" ||
              !snapshot.mode.active ||
              snapshot.mode.seeking ||
              (action === "play" &&
                completedTick(snapshot.mode.tick) + 1n >=
                  unsigned(snapshot.mode.totalTicks)) ||
              (action === "step" &&
                (snapshot.mode.playing ||
                  completedTick(snapshot.mode.tick) + 1n >=
                    unsigned(snapshot.mode.totalTicks)))}
            onclick={() => act(action as ReplayAction)}
            >{action === "live"
              ? "Return live"
              : action === "stop"
                ? "Stop / rewind"
                : action}</button
          >{/each}
        <form
          class="flex min-w-0 flex-wrap gap-2"
          onsubmit={(event) => {
            event.preventDefault();
            void act("seek");
          }}
        >
          <input
            aria-label="Exact replay target tick"
            bind:value={seekTarget}
            inputmode="text"
            class="min-w-0 max-w-56 rounded border border-border px-2 py-1.5 font-mono text-sm"
          /><button
            class="rounded border border-border px-3 py-1.5 text-sm hover:bg-muted disabled:opacity-40"
            disabled={busy ||
              view.status !== "current" ||
              !snapshot.mode.active ||
              snapshot.mode.seeking}>Seek</button
          >
        </form>
        <a href="/arena-replay" class="ml-auto text-sm text-blue-700 underline"
          >Recordings</a
        >
      </div>
      <p class="mt-2 text-xs text-muted-foreground">
        Tick −1 is the initial snapshot. Commands are confirmed by the next
        coherent observation; return-live resumes the existing live run in its
        held state.
      </p>
      {#if actionMessage}<p
          role="status"
          class={`mt-2 text-sm ${actionError ? "text-red-700" : "text-muted-foreground"}`}
        >
          {actionMessage}
        </p>{/if}
    </Panel>
    {#key contextKey(snapshot)}
      <div class="grid min-w-0 gap-4 xl:grid-cols-[minmax(0,1fr)_360px]">
        <Panel title="Arena minimap" eyebrow={`Tick ${snapshot.state.tick}`}
          ><ArenaMinimap
            {snapshot}
            {selected}
            onselect={(key) => (selected = key)}
          /></Panel
        >
        <Panel title="Entities" eyebrow="Current snapshot"
          ><ArenaEntities
            {snapshot}
            {selected}
            onselect={(key) => (selected = key)}
          /></Panel
        >
      </div>
      <div class="grid min-w-0 gap-4 xl:grid-cols-2">
        <Panel title="Game state"
          ><div class="grid grid-cols-2 gap-3 text-sm">
            {#each [["Player HP", `${snapshot.state.inventory.health}/${snapshot.state.inventory.maxHealth}`], ["Run elapsed", `${snapshot.state.elapsed.toFixed(3)} s`], ["Enemies defeated", snapshot.state.enemiesDefeated], ["Bosses defeated", snapshot.state.bossesDefeated], ["Attack multiplier", snapshot.state.inventory.attackMultiplier], ["Shield delay", `${snapshot.state.inventory.shieldDelayRemaining.toFixed(3)} s`]] as [label, value]}<div
              >
                <p class="text-xs text-muted-foreground">{label}</p>
                <p class="font-semibold">{value}</p>
              </div>{/each}
          </div>
          <div class="mt-3 text-xs">
            {#each [["Equipment", snapshot.state.inventory.equipment], ["Backpack", snapshot.state.inventory.backpack]] as const as [name, items]}<p
                class="mt-2 font-semibold"
              >
                {name}
              </p>
              {#each items as item, index}<p class="mt-1">
                  {index + 1}. {item.id
                    ? `${item.name} #${item.id} · damage ${item.damage} · shield ${item.shield} (${item.shieldFraction.toFixed(3)})`
                    : "Empty"}
                </p>{/each}{/each}
          </div>
          <details class="mt-3">
            <summary class="cursor-pointer text-xs font-medium"
              >Complete game state</summary
            >
            <pre
              class="mt-2 max-h-80 overflow-auto rounded bg-slate-50 p-3 text-xs">{JSON.stringify(
                snapshot.state,
                null,
                2,
              )}</pre>
          </details></Panel
        >
        <Panel
          title="TSQ1 presentation"
          eyebrow={`Simulation step ${snapshot.presentation.simulationStep}`}
        >
          {#each snapshot.presentation.bosses as cue (cue.id)}{@const entity =
              entities(snapshot).find(
                (v) => v.kind === "enemy" && v.id === cue.entityId,
              )}
            <div
              class="mb-3 rounded border border-orange-200 bg-orange-50 p-3 text-sm"
            >
              <div class="flex flex-wrap justify-between gap-2">
                <strong>Boss #{cue.entityId}</strong>{#if entity}<button
                    class="text-xs underline"
                    onclick={() => (selected = entity.key)}
                    >Select boss #{cue.entityId}</button
                  >{/if}
              </div>
              <p class="mt-1 text-xs">
                {cue.clipId} · offset {cue.offsetTick} · started {cue.startedTick}
              </p>
              <p class="mt-1">
                Radius {cue.radius} · intensity {cue.intensity}
              </p>
              <p class="mt-1 text-xs text-muted-foreground">
                Events delivered {cue.nextEventIndex}/{cue.eventCount}
              </p>
            </div>{/each}
          {#if snapshot.presentation.floor}{@const cue =
              snapshot.presentation.floor}
            <div class="rounded border border-blue-200 bg-blue-50 p-3 text-sm">
              <strong>Floor {cue.fromFloor} → {cue.toFloor}</strong>
              <p class="mt-1 text-xs">
                {cue.clipId} · offset {cue.offsetTick} · started {cue.startedTick}
              </p>
              <p class="mt-1">Opacity {cue.opacity}</p>
              <p class="mt-1 text-xs text-muted-foreground">
                Events delivered {cue.nextEventIndex}/{cue.eventCount}
              </p>
            </div>{/if}
          {#if !snapshot.presentation.bosses.length && !snapshot.presentation.floor}<p
              class="text-sm text-muted-foreground"
            >
              No active presentation cues.
            </p>{/if}
          <p class="mt-3 text-xs text-muted-foreground">
            Values come from the selected Runtime tick. This view does not
            advance a presentation clock.
          </p>
          <a
            class="mt-3 inline-block text-xs text-blue-700 underline"
            href="/story-sequencing">Edit presentation timelines</a
          >
        </Panel>
        <Panel title="Observed events"
          ><ArenaEvents
            {snapshot}
            onselect={(key) => (selected = key)}
            onseek={(tick) => {
              seekTarget = tick;
              void act("seek", tick);
            }}
          /></Panel
        >
        <Panel title="Tick performance"
          ><ArenaTiming timing={snapshot.timing} /></Panel
        >
      </div>
    {/key}
  {:else}<div
      class="rounded-lg border border-border bg-white p-8 text-center text-sm text-muted-foreground"
    >
      {view.status === "stale"
        ? "No valid Arena observation is available. Check the host endpoint, then refresh."
        : "Waiting for the Arena host…"}
    </div>{/if}
</div>
