type $$ComponentProps = {
    title: string;
    eyebrow?: string;
    class?: string;
    actions?: import('svelte').Snippet;
    children?: import('svelte').Snippet;
};
declare const Panel: import("svelte").Component<$$ComponentProps, {}, "">;
type Panel = ReturnType<typeof Panel>;
export default Panel;
