<script lang="ts">
  import { cn } from '../lib/cn.js'
  let { label, class: className, children }: { label: string; class?: string; children?: import('svelte').Snippet } = $props()
</script>

<label class={cn('grid gap-1.5 text-xs font-medium text-muted-foreground', className)}>
  <span>{label}</span>
  {@render children?.()}
</label>
