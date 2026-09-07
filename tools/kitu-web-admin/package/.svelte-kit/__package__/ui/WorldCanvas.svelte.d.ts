import type { WorldObject } from '../client/types.js';
export type WorldAppearance = {
    shape?: 'box' | 'sphere' | 'torus';
    scale?: number;
    color?: string;
};
type $$ComponentProps = {
    objects: WorldObject[];
    appearance?: (object: WorldObject) => WorldAppearance;
};
declare const WorldCanvas: import("svelte").Component<$$ComponentProps, {}, "">;
type WorldCanvas = ReturnType<typeof WorldCanvas>;
export default WorldCanvas;
