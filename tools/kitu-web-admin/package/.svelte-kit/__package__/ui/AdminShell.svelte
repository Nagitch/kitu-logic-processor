<script lang="ts">
  import { onDestroy, onMount, type Component } from 'svelte'
  import { Boxes } from '@lucide/svelte'
  import type { AdminClient } from '../client/create-admin-client.js'
  import { isAdminHrefActive, resolveAdminHref } from './navigation.js'
  import TransportStatus from './TransportStatus.svelte'
  export type AdminNavItem = { href: string; label: string; icon: Component<{ size?: number }> }
  export type AdminNavSection = { id: string; label: string; items: AdminNavItem[] }
  let {
    client,
    currentPath,
    sections,
    brand,
    subtitle,
    basePath = '',
    showRuntimeStatus = true,
    header,
    children
  }: {
    client: AdminClient
    currentPath: string
    sections: AdminNavSection[]
    brand: string
    subtitle: string
    basePath?: string
    showRuntimeStatus?: boolean
    header?: import('svelte').Snippet
    children?: import('svelte').Snippet
  } = $props()
  let activeSection = $derived(
    sections.find(section => section.items.some(item => isAdminHrefActive(currentPath, item.href, basePath))) ?? sections[0]
  )
  let activeItem = $derived(
    activeSection?.items.find(item => isAdminHrefActive(currentPath, item.href, basePath)) ?? activeSection?.items[0]
  )
  onMount(() => client.start())
  onDestroy(() => client.stop())
</script>

<div class="grid min-h-screen grid-cols-[240px_1fr] bg-background max-lg:grid-cols-1">
  <aside class="border-r border-border bg-white max-lg:border-b max-lg:border-r-0">
    <div class="flex h-16 items-center gap-3 border-b border-border px-4">
      <div class="flex h-9 w-9 items-center justify-center rounded-md bg-primary text-primary-foreground">
        <Boxes size={18} />
      </div>
      <div class="min-w-0">
        <p class="truncate text-sm font-semibold">{brand}</p>
        <p class="truncate text-xs text-muted-foreground">{subtitle}</p>
      </div>
    </div>
    <nav class="grid gap-3 p-3 max-lg:flex max-lg:overflow-x-auto">
      {#each sections as section, index}<div
          class={`grid gap-1 ${index > 0 ? 'border-t border-border pt-3 max-lg:border-l max-lg:border-t-0 max-lg:pl-3 max-lg:pt-0' : ''}`}>
          <p class="px-3 text-xs font-semibold uppercase text-muted-foreground">{section.label}</p>
          {#each section.items as item}{@const Icon = item.icon}
            <a
              href={resolveAdminHref(item.href, basePath)}
              class={`flex h-10 items-center gap-2 rounded-md px-3 text-sm font-medium transition-colors max-lg:min-w-36 ${isAdminHrefActive(currentPath, item.href, basePath) ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:bg-muted hover:text-foreground'}`}>
              <Icon size={16} />
              <span class="truncate">{item.label}</span>
            </a>{/each}
        </div>{/each}
    </nav>
  </aside>
  <main class="min-w-0">
    <header class="flex min-h-16 flex-wrap items-center justify-between gap-3 border-b border-border bg-white px-5 py-2">
      <div class="flex min-w-0 flex-wrap items-center gap-5">
        <div class="min-w-0">
          <p class="truncate text-xs font-medium text-muted-foreground">
            {activeSection?.label}
            <span class="px-1">/</span>
            <span class="text-foreground">{activeItem?.label}</span>
          </p>
          <p class="truncate text-sm font-semibold">{activeItem?.label}</p>
        </div>
        {#if showRuntimeStatus}<div>
            <p class="text-xs font-medium uppercase text-muted-foreground">Tick</p>
            <p class="text-sm font-semibold">{$client.worldSnapshot.tick}</p>
          </div>
          <div>
            <p class="text-xs font-medium uppercase text-muted-foreground">Objects</p>
            <p class="text-sm font-semibold">{$client.objectCount}</p>
          </div>{/if}
      </div>
      <div class="flex items-center gap-3">
        {@render header?.()}
        {#if showRuntimeStatus}<TransportStatus {client} />{/if}
      </div>
    </header>
    <div class="mx-auto max-w-7xl p-5">{@render children?.()}</div>
  </main>
</div>
