<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { page } from "$app/stores";
  import { Activity, Boxes, ScrollText } from "@lucide/svelte";
  import { Tooltip } from "bits-ui";
  import {
    connectAdminSocket,
    objectCount,
    worldSnapshot,
  } from "$lib/admin-client";
  import ConnectionBadge from "$lib/components/ConnectionBadge.svelte";

  let { children }: { children?: import("svelte").Snippet } = $props();

  const nav = [
    { href: "/", label: "Overview", icon: Activity },
    { href: "/world", label: "World", icon: Boxes },
    { href: "/logs", label: "Logs", icon: ScrollText },
  ];

  onMount(() => {
    connectAdminSocket();
  });
</script>

<Tooltip.Provider>
  <div
    class="grid min-h-screen grid-cols-[240px_1fr] bg-background max-lg:grid-cols-1"
  >
    <aside
      class="border-r border-border bg-white max-lg:border-b max-lg:border-r-0"
    >
      <div class="flex h-16 items-center gap-3 border-b border-border px-4">
        <div
          class="flex h-9 w-9 items-center justify-center rounded-md bg-primary text-primary-foreground"
        >
          <Boxes size={18} />
        </div>
        <div class="min-w-0">
          <p class="truncate text-sm font-semibold">Kitu Admin</p>
          <p class="truncate text-xs text-muted-foreground">Logic Processor</p>
        </div>
      </div>
      <nav class="grid gap-1 p-3 max-lg:grid-cols-3">
        {#each nav as item}
          {@const Icon = item.icon}
          <a
            href={item.href}
            class={`flex h-10 items-center gap-2 rounded-md px-3 text-sm font-medium transition-colors ${
              $page.url.pathname === item.href
                ? "bg-primary text-primary-foreground"
                : "text-muted-foreground hover:bg-muted hover:text-foreground"
            }`}
          >
            <Icon size={16} />
            <span class="truncate">{item.label}</span>
          </a>
        {/each}
      </nav>
    </aside>

    <main class="min-w-0">
      <header
        class="flex h-16 items-center justify-between gap-3 border-b border-border bg-white px-5"
      >
        <div class="flex min-w-0 items-center gap-4">
          <div>
            <p class="text-xs font-medium uppercase text-muted-foreground">
              Tick
            </p>
            <p class="text-sm font-semibold">{$worldSnapshot.tick}</p>
          </div>
          <div>
            <p class="text-xs font-medium uppercase text-muted-foreground">
              Objects
            </p>
            <p class="text-sm font-semibold">{$objectCount}</p>
          </div>
        </div>
        <Tooltip.Root>
          <Tooltip.Trigger>
            <ConnectionBadge />
          </Tooltip.Trigger>
          <Tooltip.Content
            class="rounded-md border border-border bg-white px-3 py-2 text-xs shadow-sm"
          >
            WebSocket status
          </Tooltip.Content>
        </Tooltip.Root>
      </header>

      <div class="mx-auto max-w-7xl p-5">
        {@render children?.()}
      </div>
    </main>
  </div>
</Tooltip.Provider>
