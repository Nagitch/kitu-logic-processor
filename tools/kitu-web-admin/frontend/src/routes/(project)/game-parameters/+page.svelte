<script lang="ts">
  import { SlidersHorizontal } from "@lucide/svelte";
  import Panel from "$lib/components/ui/Panel.svelte";
</script>

<svelte:head>
  <title>Game Parameters - Kitu Admin</title>
</svelte:head>

<Panel title="Game Parameters" eyebrow="Project">
  <div class="flex items-center justify-between gap-3 text-sm">
    <SlidersHorizontal size={18} />
    <span class="text-muted-foreground">No parameters</span>
  </div>
</Panel>
