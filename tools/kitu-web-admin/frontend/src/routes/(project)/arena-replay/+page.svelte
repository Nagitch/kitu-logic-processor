<script lang="ts">
  import { onMount } from "svelte";
  import {
    Play,
    Pause,
    Square,
    SkipForward,
    Save,
    Upload,
    RotateCcw,
  } from "@lucide/svelte";
  import Panel from "$lib/components/ui/Panel.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import {
    inspectPlayback,
    inspectRecording,
    listRecordings,
    saveRecording,
    importRecording,
    downloadRecording,
    loadRecording,
    playbackCommand,
    seekPlayback,
    type PlaybackStatus,
    type RecordingStatus,
    type RecordingFile,
  } from "$lib/arena-playback";

  let status: PlaybackStatus | null = null;
  let recording: RecordingStatus | null = null;
  let files: RecordingFile[] = [];
  let selected = "";
  let targetTick = -1;
  let busy = false;
  let refreshing = false;
  let disposed = false;
  let generation = 0;
  let error = "";
  let notice = "";
  $: mode = status?.mode;
  $: canControl = !!mode?.active && !mode.seeking && !busy;
  $: end = mode?.active && mode.tick >= mode.totalTicks - 1;

  async function refresh() {
    if (refreshing) return;
    refreshing = true;
    const id = generation;
    try {
      const [next, capture] = await Promise.all([
        inspectPlayback(),
        inspectRecording(),
      ]);
      if (!disposed && id === generation) {
        status = next;
        recording = capture;
      }
    } catch (cause) {
      if (!disposed && id === generation) error = String(cause);
    } finally {
      refreshing = false;
    }
  }
  async function refreshFiles() {
    files = await listRecordings();
    if (!files.some((file) => file.id === selected))
      selected = files[0]?.id ?? "";
  }
  async function act(action: () => Promise<void>) {
    busy = true;
    ++generation;
    error = "";
    notice = "";
    try {
      await action();
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
      await refresh();
    }
  }
  const command = (action: "play" | "pause" | "step" | "stop" | "live") =>
    act(async () => {
      status = await playbackCommand(action);
      if (action === "live")
        notice = "Live state restored. Resume explicitly in Unity when ready.";
    });
  const save = () =>
    act(async () => {
      const saved = await saveRecording();
      await refreshFiles();
      selected = saved.id;
      notice =
        "Live session saved as TSQ1. Load it to inspect its recorded ticks.";
    });
  const load = () =>
    act(async () => {
      status = await loadRecording(selected);
      targetTick = -1;
      notice =
        "Verified recording queued. The live run stays paused during replay.";
    });
  const seek = () =>
    act(async () => {
      status = await seekPlayback(targetTick);
    });
  async function upload(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    await act(async () => {
      if (file.size > 64 * 1024 * 1024)
        throw new Error("Recording exceeds the 64 MiB file limit.");
      const saved = await importRecording(await file.arrayBuffer());
      await refreshFiles();
      selected = saved.id;
      notice =
        "TSQ1 imported. Load performs full state and event verification.";
    });
    input.value = "";
  }
  onMount(() => {
    refresh();
    refreshFiles().catch((cause) => (error = String(cause)));
    const timer = setInterval(() => {
      if (!busy && document.visibilityState === "visible") refresh();
    }, 250);
    return () => {
      disposed = true;
      ++generation;
      clearInterval(timer);
    };
  });
</script>

<svelte:head><title>Arena Replay - Kitu Admin</title></svelte:head>
<div class="grid gap-4">
  <Panel title="Recordings" eyebrow="Endless Arena · TSQ1">
    <div class="grid gap-4 text-sm">
      <p class="text-muted-foreground">
        Save a live session or import a TSQ1 file, then inspect the same
        recorded state in Admin and Unity.
      </p>
      <div class="flex flex-wrap items-center gap-3">
        <Button onclick={save} disabled={busy || !!recording?.error}
          ><Save size={16} /> Save live recording</Button
        >
        <label class="flex items-center gap-2"
          ><Upload size={16} /><span>Import TSQ1</span><input
            aria-label="Import TSQ1"
            type="file"
            accept=".tsq"
            onchange={upload}
            disabled={busy}
          /></label
        >
      </div>
      {#if recording}<p class="text-muted-foreground">
          Captured {recording.ticks.toLocaleString()} live ticks · limit {recording.maxTicks.toLocaleString()}
          ticks / 64 MiB
        </p>{/if}
      {#if recording?.error}<p role="alert" class="text-red-700">
          {recording.error}
        </p>{/if}
      <div class="flex flex-wrap items-center gap-3">
        <label for="recording" class="font-medium">Saved recording</label>
        <select
          id="recording"
          bind:value={selected}
          disabled={busy}
          class="min-w-0 max-w-full rounded border border-border bg-background px-3 py-2 font-mono text-xs"
        >
          {#if !files.length}<option value="">No saved recordings</option>{/if}
          {#each files as file}<option value={file.id}
              >{file.id.slice(0, 16)}… · {(file.bytes / 1024 / 1024).toFixed(2)} MiB</option
            >{/each}
        </select>
        <Button onclick={load} disabled={busy || !selected}
          >Verify and load</Button
        >
        {#if selected}<a
            class="text-primary underline"
            href={downloadRecording(selected)}
            download="arena.tsq">Download TSQ1</a
          >{/if}
      </div>
      {#if busy}<p role="status">
          Working… Verification and seeking may take a few seconds.
        </p>{/if}
      {#if notice}<p role="status" class="text-emerald-700">{notice}</p>{/if}
      {#if error}<p role="alert" class="text-red-700">{error}</p>{/if}
    </div>
  </Panel>
  <Panel
    title="Playback"
    eyebrow={mode?.active ? "Read-only replay" : "Live run"}
  >
    <div class="grid gap-4 text-sm">
      <div class="flex flex-wrap gap-2">
        <Button
          onclick={() => command("play")}
          disabled={!canControl || mode?.playing || !!end || !!mode?.error}
          ><Play size={16} /> Play</Button
        >
        <Button
          variant="secondary"
          onclick={() => command("pause")}
          disabled={!canControl || !mode?.playing}
          ><Pause size={16} /> Pause</Button
        >
        <Button
          variant="secondary"
          onclick={() => command("step")}
          disabled={!canControl || mode?.playing || !!end || !!mode?.error}
          ><SkipForward size={16} /> Step one tick</Button
        >
        <Button
          variant="secondary"
          onclick={() => command("stop")}
          disabled={!canControl}><Square size={16} /> Stop</Button
        >
        <Button
          variant="outline"
          onclick={() => command("live")}
          disabled={busy || !mode?.active}
          ><RotateCcw size={16} /> Return to live</Button
        >
      </div>
      {#if mode?.active}
        <p aria-live="polite">
          Tick {mode.tick} of {mode.totalTicks - 1} · {mode.seeking
            ? "Seeking"
            : mode.playing
              ? "Playing"
              : end
                ? "End of recording"
                : "Paused"}
        </p>
        <div class="flex flex-wrap items-center gap-3">
          <label for="seek-tick">Seek to tick</label><input
            id="seek-tick"
            type="number"
            min="-1"
            max={mode.totalTicks - 1}
            step="1"
            bind:value={targetTick}
            class="w-36 rounded border border-border bg-background px-3 py-2"
          />
          <Button
            variant="secondary"
            onclick={seek}
            disabled={!canControl ||
              !Number.isInteger(targetTick) ||
              targetTick < -1 ||
              targetTick >= mode.totalTicks}>Seek</Button
          >
        </div>
        <p class="text-muted-foreground">
          Tick -1 is the initial state. Stop returns there. Seek re-executes
          from the beginning and leaves playback paused.
        </p>
        <p class="break-all font-mono text-xs">Recording {mode.recordingId}</p>
      {:else}<p class="text-muted-foreground">
          Load a recording to enable playback. The live run will pause at its
          next Runtime tick.
        </p>{/if}
      {#if mode?.error}<p role="alert" class="text-red-700">
          {mode.error}
        </p>{/if}
      {#if status}
        <dl class="grid grid-cols-2 gap-2 sm:grid-cols-4">
          <div>
            <dt class="text-muted-foreground">Floor</dt>
            <dd class="font-semibold">{status.state.floor}F</dd>
          </div>
          <div>
            <dt class="text-muted-foreground">HP</dt>
            <dd class="font-semibold">
              {status.state.inventory.health} / {status.state.inventory
                .maxHealth}
            </dd>
          </div>
          <div>
            <dt class="text-muted-foreground">Enemies</dt>
            <dd class="font-semibold">{status.state.enemies.length}</dd>
          </div>
          <div>
            <dt class="text-muted-foreground">Game time</dt>
            <dd class="font-semibold">{status.state.elapsed.toFixed(2)} s</dd>
          </div>
        </dl>
        <p class="break-all font-mono text-xs">Content {status.contentHash}</p>
        <details>
          <summary>Execution version and complete state</summary>
          <pre
            class="mt-2 max-h-96 overflow-auto rounded bg-muted p-3 text-xs">{JSON.stringify(
              {
                execution: status.execution,
                liveTick: status.liveTick,
                state: status.state,
              },
              null,
              2,
            )}</pre>
        </details>
      {/if}
    </div>
  </Panel>
</div>
