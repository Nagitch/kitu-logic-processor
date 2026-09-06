<script lang="ts">
  import { onMount } from "svelte";
  import { Check, FileCheck, RefreshCw } from "@lucide/svelte";
  import Panel from "$lib/components/ui/Panel.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import {
    inspectScript,
    validateScript,
    stageScript,
    type ScriptDiagnostic,
    type ScriptStatus,
  } from "$lib/arena-script";

  let status: ScriptStatus | null = null;
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

  function location(diagnostic: ScriptDiagnostic) {
    if (diagnostic.line === null) return "";
    return `Line ${diagnostic.line}${diagnostic.column === null ? "" : `, column ${diagnostic.column}`}`;
  }

  async function refresh() {
    if (busy) return;
    const id = ++requestId;
    try {
      const next = await inspectScript();
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
      const next = await validateScript();
      if (!disposed && id === requestId) {
        status = next;
        selected = "candidate";
        if (next.candidate) notice = "Validated. Review the script below.";
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
      const result = await stageScript(candidate);
      if (disposed || id !== requestId) return;
      notice = `Queued for the next run (command ${result.sequence}). The current run keeps its saved script.`;
      const next = await inspectScript();
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

<svelte:head><title>Game Scripts - Kitu Admin</title></svelte:head>

<div class="grid gap-4">
  <Panel title="Endless Arena boss script" eyebrow="Game scripts">
    <div class="grid gap-4 text-sm">
      <p class="text-muted-foreground">
        Edit and save the configured Rhai source file, reload and validate it
        here, then apply the reviewed script to the next run. The source below
        is read-only.
      </p>
      {#if status?.readOnly}
        <p class="text-amber-700">
          Replay inspection is read-only. Return to the live run before applying
          a script.
        </p>
      {/if}
      {#if status}
        <div>
          <p class="text-muted-foreground">Configured source</p>
          <p class="break-all font-mono text-xs">
            {status.path ?? "Bundled default"}
          </p>
          {#if status.path === null}
            <p class="mt-1 text-muted-foreground">
              Configure an external script file on the host to edit boss
              behavior.
            </p>
          {/if}
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
          <p>The current and next run keep their last valid scripts.</p>
          {#each status.diagnostics as diagnostic}
            <div class="mt-3">
              <p class="font-medium">
                {diagnostic.kind}
                {#if location(diagnostic)}
                  <span class="font-normal">· {location(diagnostic)}</span>
                {/if}
              </p>
              <p class="whitespace-pre-wrap break-words font-mono text-xs">
                {diagnostic.message}
              </p>
            </div>
          {/each}
        </div>
      {/if}
      {#if status}
        <dl class="grid gap-4 rounded-md border border-border p-3">
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
            <dt class="text-muted-foreground">Validated edit</dt>
            <dd class="break-all font-mono text-xs">
              {status.candidate?.hash ?? "No validated edit"}
            </dd>
          </div>
        </dl>
      {:else if !error}
        <p role="status" class="text-muted-foreground">
          Loading script status…
        </p>
      {/if}
    </div>
  </Panel>

  {#if status?.runtime.fault}
    {@const fault = status.runtime.fault}
    <Panel title="Current run script fault" eyebrow="Runtime diagnostic">
      <div role="alert" class="grid gap-2 text-sm">
        <p>
          The run is paused and cannot resume. Stage a valid script, return to
          the menu, and start a new run.
        </p>
        <p class="text-muted-foreground">
          Tick {fault.tick} · Enemy {fault.enemyId}
        </p>
        <p class="break-all font-mono text-xs">{fault.scriptHash}</p>
        <p class="font-medium">
          {fault.diagnostic.kind}
          {#if location(fault.diagnostic)}
            <span class="font-normal">· {location(fault.diagnostic)}</span>
          {/if}
        </p>
        <p class="whitespace-pre-wrap break-words font-mono text-xs">
          {fault.diagnostic.message}
        </p>
      </div>
    </Panel>
  {/if}

  <Panel title="Script source" eyebrow="Review">
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
        <dl class="grid gap-3">
          <div>
            <dt class="text-muted-foreground">Script version</dt>
            <dd class="break-all font-mono text-xs">{version.hash}</dd>
          </div>
          <div>
            <dt class="text-muted-foreground">Source digest</dt>
            <dd class="break-all font-mono text-xs">
              {version.sourceSha256}
            </dd>
          </div>
        </dl>
        <details class="rounded-md border border-border p-3">
          <summary class="cursor-pointer font-medium">
            Execution versions
          </summary>
          <dl class="mt-3 grid gap-2 sm:grid-cols-3">
            <div>
              <dt class="text-muted-foreground">Script contract</dt>
              <dd class="font-mono text-xs">{version.contractVersion}</dd>
            </div>
            <div>
              <dt class="text-muted-foreground">Execution policy</dt>
              <dd class="break-all font-mono text-xs">
                {version.policyVersion}
              </dd>
            </div>
            <div>
              <dt class="text-muted-foreground">Rhai</dt>
              <dd class="font-mono text-xs">{version.rhaiVersion}</dd>
            </div>
          </dl>
        </details>
        <label class="grid gap-2 font-medium">
          Rhai source · read-only
          <textarea
            readonly
            value={version.source}
            rows="20"
            wrap="off"
            spellcheck="false"
            class="max-h-[32rem] min-h-40 w-full resize-y overflow-auto rounded-md border border-border bg-muted p-4 font-mono text-xs font-normal leading-5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          ></textarea>
        </label>
      {:else}
        <p class="text-muted-foreground">
          {selected === "active"
            ? "No run has started yet."
            : "No validated edit. Reload and validate the source or select another version."}
        </p>
      {/if}
    </div>
  </Panel>
</div>
