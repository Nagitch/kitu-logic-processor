import type { WorldObject } from '../client/types.js';
import { type WorldAppearance } from '../ui/WorldCanvas.svelte';
export type WorldKind = {
    value: string;
    label: string;
};
type $$ComponentProps = {
    kinds: WorldKind[];
    appearance?: (object: WorldObject) => WorldAppearance;
};
declare const WorldPage: import("svelte").Component<$$ComponentProps, {}, "">;
type WorldPage = ReturnType<typeof WorldPage>;
export default WorldPage;
