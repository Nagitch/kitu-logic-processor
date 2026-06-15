<script lang="ts">
  import { Map } from "@lucide/svelte";
  import Panel from "$lib/components/ui/Panel.svelte";
</script>

<svelte:head>
  <title>Level Designer - Kitu Admin</title>
</svelte:head>

<Panel title="Level Designer" eyebrow="Project">
  <div class="flex items-center justify-between gap-3 text-sm">
    <Map size={18} />
    <span class="text-muted-foreground">No level data</span>
  </div>
</Panel>
