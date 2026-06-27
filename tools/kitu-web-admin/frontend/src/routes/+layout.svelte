<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { page } from "$app/stores";
  import {
    Activity,
    Boxes,
    Bolt,
    BookOpen,
    Map,
    ScrollText,
    SlidersHorizontal,
  } from "@lucide/svelte";
  import {
    connectAdminSocket,
    objectCount,
    worldSnapshot,
  } from "$lib/admin-client";
  import TransportStatus from "$lib/components/TransportStatus.svelte";

  let { children }: { children?: import("svelte").Snippet } = $props();

  const navSections = [
    {
      id: "kitu-general",
      label: "Kitu general",
      items: [
        { href: "/", label: "Overview", icon: Activity },
        { href: "/world", label: "World", icon: Boxes },
        { href: "/logs", label: "Logs", icon: ScrollText },
      ],
    },
    {
      id: "project",
      label: "Project",
      items: [
        { href: "/app-actions", label: "App Actions", icon: Bolt },
        { href: "/level-designer", label: "Level Designer", icon: Map },
        {
          href: "/story-sequencing",
          label: "Story Sequencing",
          icon: BookOpen,
        },
        {
          href: "/game-parameters",
          label: "Game Parameters",
          icon: SlidersHorizontal,
        },
      ],
    },
  ];

  let activeSection = $derived(
    navSections.find((section) =>
      section.items.some((item) => item.href === $page.url.pathname),
    ) ?? navSections[0],
  );
  let activeItem = $derived(
    activeSection.items.find((item) => item.href === $page.url.pathname) ??
      activeSection.items[0],
  );

  onMount(() => {
    connectAdminSocket();
  });
</script>

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
    <nav class="grid gap-3 p-3 max-lg:flex max-lg:overflow-x-auto">
      {#each navSections as section, index}
        <div
          class={`grid gap-1 ${index > 0 ? "border-t border-border pt-3 max-lg:border-l max-lg:border-t-0 max-lg:pl-3 max-lg:pt-0" : ""}`}
        >
          <p class="px-3 text-xs font-semibold uppercase text-muted-foreground">
            {section.label}
          </p>
          {#each section.items as item}
            {@const Icon = item.icon}
            <a
              href={item.href}
              class={`flex h-10 items-center gap-2 rounded-md px-3 text-sm font-medium transition-colors max-lg:min-w-36 ${
                $page.url.pathname === item.href
                  ? "bg-primary text-primary-foreground"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground"
              }`}
            >
              <Icon size={16} />
              <span class="truncate">{item.label}</span>
            </a>
          {/each}
        </div>
      {/each}
    </nav>
  </aside>

  <main class="min-w-0">
    <header
      class="flex min-h-16 flex-wrap items-center justify-between gap-3 border-b border-border bg-white px-5 py-2"
    >
      <div class="flex min-w-0 flex-wrap items-center gap-5">
        <div class="min-w-0">
          <p class="truncate text-xs font-medium text-muted-foreground">
            {activeSection.label}
            <span class="px-1">/</span>
            <span class="text-foreground">{activeItem.label}</span>
          </p>
          <p class="truncate text-sm font-semibold">{activeItem.label}</p>
        </div>
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
      <TransportStatus />
    </header>

    <div class="mx-auto max-w-7xl p-5">
      {@render children?.()}
    </div>
  </main>
</div>
