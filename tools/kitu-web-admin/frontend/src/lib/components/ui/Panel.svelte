<script lang="ts">
  import { cn } from "$lib/utils";

  let {
    title,
    eyebrow,
    class: className,
    actions,
    children,
  }: {
    title: string;
    eyebrow?: string;
    class?: string;
    actions?: import("svelte").Snippet;
    children?: import("svelte").Snippet;
  } = $props();
</script>

<section class={cn("rounded-lg border border-border bg-white", className)}>
  <header
    class="flex min-h-14 items-center justify-between gap-3 border-b border-border px-4"
  >
    <div class="min-w-0">
      {#if eyebrow}
        <p class="text-xs font-medium uppercase text-muted-foreground">
          {eyebrow}
        </p>
      {/if}
      <h2 class="truncate text-sm font-semibold">{title}</h2>
    </div>
    {#if actions}
      <div class="flex shrink-0 items-center gap-2">
        {@render actions()}
      </div>
    {/if}
  </header>
  <div class="p-4">
    {@render children?.()}
  </div>
</section>
