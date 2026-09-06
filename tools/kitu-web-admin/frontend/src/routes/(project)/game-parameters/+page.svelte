<script lang="ts">
  import { onMount } from "svelte";
  import { Check, FileCheck, RefreshCw } from "@lucide/svelte";
  import Panel from "$lib/components/ui/Panel.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import {
    inspectContent,
    validateContent,
    stageContent,
    type ContentStatus,
  } from "$lib/arena-content";

  let status: ContentStatus | null = null;
  let busy = false;
  let error = "";
  let notice = "";
  let selected = "candidate";
  let requestId = 0;
  let disposed = false;

  $: version =
    selected === "active"
      ? status?.runtime.active
      : selected === "pending"
        ? status?.runtime.pending
        : status?.candidate;
  $: tables = version
    ? [
        { name: "Items", rows: version.values.items },
        { name: "Enemies", rows: version.values.enemies },
        { name: "Difficulty", rows: [version.values.difficulty] },
        { name: "Chests", rows: version.values.chests },
      ]
    : [];
  $: alreadyStaged =
    status?.candidate?.sourceSha256 === status?.runtime.pending.sourceSha256 &&
    status?.candidate?.hash === status?.runtime.pending.hash;

  async function refresh() {
    const id = ++requestId;
    try {
      const next = await inspectContent();
      if (!disposed && id === requestId) {
        status = next;
        error = "";
      }
    } catch (cause) {
      if (!disposed && id === requestId) error = String(cause);
    }
  }

  async function validate() {
    busy = true;
    ++requestId;
    notice = "";
    error = "";
    try {
      status = await validateContent();
      selected = "candidate";
      if (status.candidate) notice = "Validated. Review the values below.";
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }

  async function apply() {
    if (!status?.candidate) return;
    busy = true;
    ++requestId;
    error = "";
    try {
      const result = await stageContent(status.candidate);
      notice = `Queued for the next run (command ${result.sequence}). The current run keeps its saved values.`;
      await refresh();
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
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

<svelte:head><title>Game Parameters - Kitu Admin</title></svelte:head>

<div class="grid gap-4">
  <Panel title="Endless Arena parameters" eyebrow="Tanu content">
    <div class="grid gap-4 text-sm">
      <p class="text-muted-foreground">
        Edit and save the Tanu tables in VS Code, validate the file here, then
        apply the evaluated values to the next run.
      </p>
      {#if status?.readOnly}<p class="text-amber-700">
          Replay inspection is read-only. Return to the live run before applying
          parameters.
        </p>{/if}
      {#if status}<p class="break-all font-mono text-xs">{status.path}</p>{/if}
      <div class="flex flex-wrap gap-2">
        <Button onclick={validate} disabled={busy}
          ><FileCheck size={16} /> {busy ? "Working…" : "Validate TMD"}</Button
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
          <p>The last valid settings remain available for the next run.</p>
          {#each status.diagnostics as diagnostic}<p
              class="mt-2 whitespace-pre-wrap break-words font-mono text-xs"
            >
              {diagnostic}
            </p>{/each}
        </div>
      {/if}
      {#if status?.persistenceError}<p role="alert" class="text-destructive">
          Run settings could not be saved: {status.persistenceError}
        </p>{/if}
      {#if status}
        <dl class="grid gap-3 rounded-md border border-border p-3">
          <div>
            <dt class="text-muted-foreground">
              Current run · {status.runtime.run}
            </dt>
            <dd class="break-all font-mono text-xs">
              {status.runtime.active?.hash ?? "No run started"}
            </dd>
          </div>
          <div>
            <dt class="text-muted-foreground">Next run</dt>
            <dd class="break-all font-mono text-xs">
              {status.runtime.pending.hash}
            </dd>
          </div>
          <div>
            <dt class="text-muted-foreground">Last saved run settings</dt>
            <dd>{status.savedRun ?? "No run saved yet"}</dd>
          </div>
        </dl>
      {/if}
    </div>
  </Panel>
  <Panel title="Evaluated values" eyebrow="Review">
    <div class="grid gap-4">
      <label class="flex items-center gap-3 text-sm"
        >Configuration
        <select
          bind:value={selected}
          class="rounded-md border border-border bg-background p-2"
        >
          <option value="candidate">Validated edit</option><option
            value="pending">Next run</option
          ><option value="active">Current run</option>
        </select>
      </label>
      {#if version}
        <p class="break-all font-mono text-xs text-muted-foreground">
          {version.hash}
        </p>
        {#each tables as table}
          <div class="overflow-x-auto">
            <table class="w-full text-left text-xs">
              <caption class="py-2 text-left text-sm font-semibold"
                >{table.name}</caption
              >
              <thead
                ><tr
                  >{#each Object.keys(table.rows[0] ?? {}) as column}<th
                      class="border-b border-border px-3 py-2 font-medium"
                      >{column}</th
                    >{/each}</tr
                ></thead
              >
              <tbody
                >{#each table.rows as row}<tr
                    >{#each Object.values(row) as value}<td
                        class="border-b border-border px-3 py-2">{value}</td
                      >{/each}</tr
                  >{:else}<tr
                    ><td class="p-3 text-muted-foreground">No entries</td></tr
                  >{/each}</tbody
              >
            </table>
          </div>
        {/each}
      {:else}<p class="text-sm text-muted-foreground">
          No configuration selected. Validate the TMD or select another version.
        </p>{/if}
    </div>
  </Panel>
</div>
