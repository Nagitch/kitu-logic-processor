import type { HTMLButtonAttributes } from 'svelte/elements';
type Variant = 'default' | 'secondary' | 'outline' | 'destructive' | 'ghost';
type Size = 'default' | 'sm' | 'icon';
type $$ComponentProps = HTMLButtonAttributes & {
    variant?: Variant;
    size?: Size;
    children?: import('svelte').Snippet;
};
declare const Button: import("svelte").Component<$$ComponentProps, {}, "">;
type Button = ReturnType<typeof Button>;
export default Button;
