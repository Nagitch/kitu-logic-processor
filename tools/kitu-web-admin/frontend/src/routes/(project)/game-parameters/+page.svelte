<script lang="ts">
  import { onMount } from "svelte";
  import { Check, FileCheck, RefreshCw } from "@lucide/svelte";
  import { Button, Panel } from "@kitu/admin/ui";
  import {
    inspectContent,
    validateContent,
    stageContent,
    type ContentLayer,
    type ContentStatus,
  } from "$lib/arena-content";

  let status: ContentStatus | null = null;
  let busy = false;
  let error = "";
  let notice = "";
  let selected: "candidate" | "pending" | "active" = "candidate";
  let comparison: "active" | "pending" = "pending";
  let originFilter = "";
  let requestId = 0;
  let disposed = false;

  const layers: ContentLayer[] = ["base", "difficulty", "event", "debug"];
  const layerNames: Record<ContentLayer, string> = {
    base: "Base",
    difficulty: "Difficulty",
    event: "Event",
    debug: "Debug",
  };

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
  $: sources = layers.flatMap((layer) =>
    (status?.sources ?? []).filter((source) => source.layer === layer),
  );
  $: differences = status?.differences?.[comparison];
  $: comparisonName = comparison === "active" ? "current run" : "next run";
  $: origins =
    status?.origins?.[selected] ?? version?.provenance?.origins ?? null;
  $: visibleOrigins = Object.entries(origins ?? {})
    .sort(([left], [right]) => left.localeCompare(right))
    .filter(([path, layer]) =>
      `${parameterName(path)} ${layerNames[layer]}`
        .toLowerCase()
        .includes(originFilter.toLowerCase()),
    );

  function parameterName(path: string) {
    return path
      .split("/")
      .slice(1)
      .map((part) => part.replace(/~1/g, "/").replace(/~0/g, "~"))
      .join(" / ");
  }

  function displayValue(value: unknown) {
    if (value === null) return "Not present";
    if (typeof value === "object") return JSON.stringify(value, null, 2);
    return String(value);
  }

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
  <Panel title="Endless Arena parameters" eyebrow="Game content">
    <div class="grid gap-4 text-sm">
      <p class="text-muted-foreground">
        Edit and save the configured Tanu tables or SQLite data, reload and
        validate them here, then apply the evaluated values to the next run.
      </p>
      {#if status?.readOnly}<p class="text-amber-700">
          Replay inspection is read-only. Return to the live run before applying
          parameters.
        </p>{/if}
      {#if status}
        <div>
          <p class="text-muted-foreground">Configured source</p>
          <p class="break-all font-mono text-xs">{status.path}</p>
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
          <p>The current and next run keep their last valid settings.</p>
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
  <Panel title="Validated sources" eyebrow="Override order">
    <div class="grid gap-4 text-sm">
      <p class="text-muted-foreground">
        Later configured layers win when they set the same value. Layers that
        are not configured are skipped.
      </p>
      <ol
        aria-label="Configuration override order"
        class="flex flex-wrap gap-2"
      >
        {#each layers as layer, index}
          <li class="rounded-md border border-border px-3 py-2">
            {index + 1}. {layerNames[layer]}
          </li>
        {/each}
      </ol>
      {#if sources.length}
        <div class="overflow-x-auto">
          <table class="w-full text-left text-xs">
            <caption class="pb-2 text-left text-muted-foreground">
              Source files used for the validated edit
            </caption>
            <thead>
              <tr>
                {#each ["Layer", "Format", "Path", "Source digest"] as heading}
                  <th
                    scope="col"
                    class="border-b border-border px-3 py-2 font-medium"
                    >{heading}</th
                  >
                {/each}
              </tr>
            </thead>
            <tbody>
              {#each sources as source}
                <tr>
                  <td class="border-b border-border px-3 py-2"
                    >{layerNames[source.layer]}</td
                  >
                  <td class="border-b border-border px-3 py-2"
                    >{source.format === "tmd" ? "TMD" : "SQLite"}</td
                  >
                  <td
                    class="max-w-sm break-all border-b border-border px-3 py-2 font-mono"
                    >{source.path}</td
                  >
                  <td
                    class="max-w-xs break-all border-b border-border px-3 py-2 font-mono"
                    >{source.sourceSha256}</td
                  >
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {:else}
        <p class="text-muted-foreground">
          No validated source list. Reload and validate the configured source to
          inspect its layers.
        </p>
      {/if}
    </div>
  </Panel>
  <Panel title="Changes in validated edit" eyebrow="Before applying">
    <div class="grid gap-4 text-sm">
      <label class="flex items-center gap-3">
        Compare with
        <select
          bind:value={comparison}
          class="rounded-md border border-border bg-background p-2"
        >
          <option value="pending">Next run</option>
          <option value="active">Current run</option>
        </select>
      </label>
      {#if !status?.candidate}
        <p class="text-muted-foreground">
          Reload and validate an edit to compare its values.
        </p>
      {:else if differences == null}
        <p class="text-muted-foreground">
          No {comparisonName} configuration is available to compare.
        </p>
      {:else if differences.length === 0}
        <p class="text-muted-foreground">
          The validated values match the {comparisonName} settings.
        </p>
      {:else}
        <div class="overflow-x-auto">
          <table class="w-full text-left text-xs">
            <caption class="pb-2 text-left text-muted-foreground">
              {differences.length} changed {differences.length === 1
                ? "value"
                : "values"} compared with the {comparisonName}
            </caption>
            <thead>
              <tr>
                {#each ["Parameter", "Before", "Validated edit", "Winning layer"] as heading}
                  <th
                    scope="col"
                    class="border-b border-border px-3 py-2 font-medium"
                    >{heading}</th
                  >
                {/each}
              </tr>
            </thead>
            <tbody>
              {#each differences as difference}
                <tr>
                  <th
                    scope="row"
                    class="max-w-xs break-words border-b border-border px-3 py-2 font-medium"
                    >{parameterName(difference.path)}</th
                  >
                  <td
                    class="max-w-sm whitespace-pre-wrap break-words border-b border-border px-3 py-2 font-mono"
                    >{displayValue(difference.before)}</td
                  >
                  <td
                    class="max-w-sm whitespace-pre-wrap break-words border-b border-border px-3 py-2 font-mono"
                    >{displayValue(difference.after)}</td
                  >
                  <td class="border-b border-border px-3 py-2"
                    >{layerNames[difference.winningLayer]}</td
                  >
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
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
        <dl class="grid gap-3 text-sm">
          <div>
            <dt class="text-muted-foreground">
              Evaluated configuration version
            </dt>
            <dd class="break-all font-mono text-xs">{version.hash}</dd>
          </div>
          <div>
            <dt class="text-muted-foreground">Source digest</dt>
            <dd class="break-all font-mono text-xs">{version.sourceSha256}</dd>
          </div>
        </dl>
        <details class="rounded-md border border-border p-3 text-sm">
          <summary class="cursor-pointer font-medium">Value origins</summary>
          <div class="mt-3 grid gap-3">
            {#if origins}
              <label class="grid gap-1">
                Filter parameters or layers
                <input
                  bind:value={originFilter}
                  type="search"
                  class="rounded-md border border-border bg-background p-2"
                />
              </label>
              <div class="max-h-80 overflow-auto">
                <table class="w-full text-left text-xs">
                  <caption class="pb-2 text-left text-muted-foreground"
                    >Winning layer for the selected configuration</caption
                  >
                  <thead
                    ><tr>
                      <th
                        scope="col"
                        class="border-b border-border px-3 py-2 font-medium"
                        >Parameter</th
                      >
                      <th
                        scope="col"
                        class="border-b border-border px-3 py-2 font-medium"
                        >Winning layer</th
                      >
                    </tr></thead
                  >
                  <tbody>
                    {#each visibleOrigins as [path, layer]}
                      <tr>
                        <th
                          scope="row"
                          class="break-words border-b border-border px-3 py-2 font-medium"
                          >{parameterName(path)}</th
                        >
                        <td class="border-b border-border px-3 py-2"
                          >{layerNames[layer]}</td
                        >
                      </tr>
                    {:else}
                      <tr
                        ><td colspan="2" class="p-3 text-muted-foreground"
                          >No matching parameters.</td
                        ></tr
                      >
                    {/each}
                  </tbody>
                </table>
              </div>
            {:else}
              <p class="text-muted-foreground">
                Field origins were not saved with this configuration.
              </p>
            {/if}
          </div>
        </details>
        {#each tables as table}
          <div class="overflow-x-auto">
            <table class="w-full text-left text-xs">
              <caption class="py-2 text-left text-sm font-semibold"
                >{table.name}</caption
              >
              <thead
                ><tr
                  >{#each Object.keys(table.rows[0] ?? {}) as column}<th
                      scope="col"
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
          No configuration selected. Reload and validate the source or select
          another version.
        </p>{/if}
    </div>
  </Panel>
</div>
