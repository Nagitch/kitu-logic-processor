<script lang="ts">
  import type { HTMLButtonAttributes } from "svelte/elements";
  import { cn } from "$lib/utils";

  type Variant = "default" | "secondary" | "outline" | "destructive" | "ghost";
  type Size = "default" | "sm" | "icon";

  let {
    class: className,
    variant = "default",
    size = "default",
    type = "button",
    children,
    ...rest
  }: HTMLButtonAttributes & {
    variant?: Variant;
    size?: Size;
    children?: import("svelte").Snippet;
  } = $props();

  const variants: Record<Variant, string> = {
    default: "bg-primary text-primary-foreground hover:bg-primary/90",
    secondary: "bg-secondary text-secondary-foreground hover:bg-secondary/90",
    outline: "border border-border bg-background hover:bg-muted",
    destructive:
      "bg-destructive text-destructive-foreground hover:bg-destructive/90",
    ghost: "hover:bg-muted",
  };

  const sizes: Record<Size, string> = {
    default: "h-10 px-4",
    sm: "h-8 px-3 text-xs",
    icon: "h-9 w-9 p-0",
  };
</script>

<button
  class={cn(
    "inline-flex shrink-0 items-center justify-center gap-2 rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50",
    variants[variant],
    sizes[size],
    className,
  )}
  {type}
  {...rest}
>
  {@render children?.()}
</button>
