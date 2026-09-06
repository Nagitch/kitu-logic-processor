<script lang="ts">
  import { onMount } from "svelte";
  import { Terminal, Send, RotateCcw } from "@lucide/svelte";
  import Panel from "$lib/components/ui/Panel.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import { apiBaseUrl } from "$lib/admin-client";
  type Spec = {
    prefix: string[];
    usage: string;
    description: string;
    mutates: boolean;
  };
  type Catalog = { version: number; sessionId: string; commands: Spec[] };
  type Request = {
    version: number;
    sessionId: string;
    clientId: string;
    id: number;
    line: string;
  };
  type Result = {
    id: number;
    ok: boolean;
    data: unknown;
    error: string | null;
  };
  let catalog: Catalog | null = null;
  let clientId = "";
  let nextId = 1;
  let line = "inspect application";
  let busy = false;
  let error = "";
  let pending: Request | null = null;
  let history: { request: Request; result: Result }[] = [];
  let choices: string[] = [];
  let historyIndex = -1;

  async function connect() {
    busy = true;
    error = "";
    try {
      const response = await fetch(`${apiBaseUrl()}/shell/catalog`);
      if (!response.ok)
        throw new Error(`Connection failed: ${response.status}`);
      const next: Catalog = await response.json();
      if (next.version !== 1)
        throw new Error("Incompatible Shell command version.");
      if (pending && pending.sessionId !== next.sessionId)
        throw new Error(
          "The host restarted. The previous request cannot be retried against this new session.",
        );
      catalog = next;
    } catch (cause) {
      error = String(cause);
    } finally {
      busy = false;
    }
  }
  async function execute(request: Request) {
    busy = true;
    error = "";
    try {
      const response = await fetch(`${apiBaseUrl()}/shell/line`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(request),
      });
      if (!response.ok) throw new Error(`Request failed: ${response.status}`);
      const result: Result = await response.json();
      history = [{ request, result }, ...history].slice(0, 50);
      pending = null;
    } catch (cause) {
      pending = request;
      error = `${String(cause)} The outcome may already have applied. Retry uses exactly the same request identity.`;
    } finally {
      busy = false;
    }
  }
  function send(event: SubmitEvent) {
    event.preventDefault();
    if (!catalog || busy || pending || !line.trim()) return;
    choices = [line, ...choices].slice(0, 50);
    historyIndex = -1;
    execute({
      version: catalog.version,
      sessionId: catalog.sessionId,
      clientId,
      id: nextId++,
      line,
    });
  }
  function historyKey(event: KeyboardEvent) {
    if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      historyIndex = Math.min(
        choices.length - 1,
        Math.max(-1, historyIndex + (event.key === "ArrowUp" ? 1 : -1)),
      );
      if (historyIndex >= 0) line = choices[historyIndex];
    }
  }
  onMount(() => {
    clientId = `browser-${crypto.randomUUID()}`;
    connect();
  });
</script>

<svelte:head><title>Shell - Kitu Admin</title></svelte:head>
<div class="grid gap-4">
  <Panel title="Kitu Shell" eyebrow="Running Runtime">
    <div class="grid gap-4 text-sm">
      <p>
        Inspect and operate the same live run as Unity. Commands and quoting are
        interpreted by Kitu's shared command service.
      </p>
      <div class="flex flex-wrap items-center gap-3">
        <Button onclick={connect} disabled={busy}
          ><RotateCcw size={16} /> Reconnect</Button
        >
        <span class="break-all font-mono text-xs text-muted-foreground"
          >{catalog?.sessionId ?? "Connecting…"}</span
        >
      </div>
      <form onsubmit={send} class="flex flex-wrap items-end gap-3">
        <label class="grid min-w-0 flex-1 gap-1 font-medium" for="shell-command"
          >Command
          <input
            id="shell-command"
            bind:value={line}
            onkeydown={historyKey}
            disabled={busy || !!pending}
            autocomplete="off"
            spellcheck="false"
            class="w-full rounded border border-border bg-background px-3 py-2 font-mono text-sm"
          />
        </label>
        <Button
          type="submit"
          disabled={!catalog || busy || !!pending || !line.trim()}
          ><Send size={16} /> Run</Button
        >
      </form>
      {#if busy}<p role="status">Executing on Runtime…</p>{/if}
      {#if error}<p role="alert" class="text-red-700">{error}</p>{/if}
      {#if pending}<div class="flex flex-wrap items-center gap-3">
          <Button onclick={() => pending && execute(pending)} disabled={busy}
            >Retry same request</Button
          >
          <Button
            onclick={() => {
              pending = null;
              error = "";
            }}
            disabled={busy}>Continue without retry</Button
          >
        </div>{/if}
      <p class="text-xs text-muted-foreground">
        Use help to inspect all commands. For example: app action run
        arena.start · app action run arena.pause · replay step. Up/Down recalls
        command history. An uncertain request is never automatically repeated
        with a new ID.
      </p>
    </div>
  </Panel>
  <div
    class="grid grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)] gap-4 max-xl:grid-cols-1"
  >
    <Panel title="Command results" eyebrow="Applied results and refusals">
      <div class="grid gap-3">
        {#each history as entry}
          <article class="min-w-0 rounded border border-border p-3 text-sm">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <code class="break-all">{entry.request.line}</code>
              <span
                class={entry.result.ok
                  ? "font-semibold text-emerald-700"
                  : "font-semibold text-red-700"}
                >{entry.result.ok ? "Completed" : "Refused"} · #{entry.result
                  .id}</span
              >
            </div>
            {#if entry.result.error}<p class="mt-2 text-red-700">
                {entry.result.error}
              </p>{/if}
            <details class="mt-2" open={entry === history[0]}>
              <summary class="cursor-pointer text-muted-foreground"
                >Result and state</summary
              >
              <pre
                class="mt-2 max-h-96 overflow-auto rounded bg-muted p-3 text-xs">{JSON.stringify(
                  entry.result.data,
                  null,
                  2,
                )}</pre>
            </details>
          </article>
        {:else}<p class="text-sm text-muted-foreground">
            Run a command to see its result.
          </p>{/each}
      </div>
    </Panel>
    <Panel title="Shared commands" eyebrow="From the connected host">
      <div class="grid gap-3 text-sm">
        {#each catalog?.commands ?? [] as spec}
          <div class="border-b border-border pb-3 last:border-0">
            <p class="flex items-start gap-2">
              <Terminal size={15} class="mt-0.5 shrink-0" /><code
                class="break-words">{spec.usage}</code
              >
            </p>
            <p class="mt-1 text-muted-foreground">{spec.description}</p>
          </div>
        {/each}
      </div>
    </Panel>
  </div>
</div>
