<script lang="ts">
  import { connectionState } from "$lib/admin-client";

  const tone = {
    idle: "bg-muted text-muted-foreground",
    connecting: "bg-secondary text-secondary-foreground",
    open: "bg-accent text-accent-foreground",
    closed: "bg-muted text-muted-foreground",
    error: "bg-destructive text-destructive-foreground",
  };
</script>

<span
  class={`inline-flex h-8 items-center rounded-md px-3 text-xs font-semibold ${tone[$connectionState]}`}
>
  {$connectionState}
</span>
