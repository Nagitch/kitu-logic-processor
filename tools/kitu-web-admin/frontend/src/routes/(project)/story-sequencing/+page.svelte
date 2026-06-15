<script lang="ts">
  import { BookOpen } from "@lucide/svelte";
  import Panel from "$lib/components/ui/Panel.svelte";
</script>

<svelte:head>
  <title>Story Sequencing - Kitu Admin</title>
</svelte:head>

<Panel title="Story Sequencing" eyebrow="Project">
  <div class="flex items-center justify-between gap-3 text-sm">
    <BookOpen size={18} />
    <span class="text-muted-foreground">No sequences</span>
  </div>
</Panel>
