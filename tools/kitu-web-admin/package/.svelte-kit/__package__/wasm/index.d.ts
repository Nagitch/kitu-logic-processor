import type { ClientOscMessage } from '../client/types.js';
export interface OscIrWasmModule {
    default(input?: string | URL | Request): Promise<unknown>;
    admin_world_spawn(kind: string, x: number, y: number, z: number): unknown;
    admin_world_move(id: string, x: number, y: number, z: number): unknown;
    admin_world_reset(): unknown;
}
export interface OscIrLoaderOptions {
    /** Application base URL, normally derived from the consuming router's base path. */
    baseUrl?: string | URL;
    /** URL to the generated JS module. Relative URLs resolve against baseUrl. */
    moduleUrl?: string | URL;
    /** Optional explicit .wasm URL passed to wasm-bindgen's initializer. */
    wasmUrl?: string | URL;
    importModule?: (url: string) => Promise<unknown>;
}
export declare function createOscIrLoader(options?: OscIrLoaderOptions): {
    initialize: () => Promise<void>;
    spawn: (kind: string, x: number, y: number, z: number) => Promise<ClientOscMessage>;
    move: (id: string, x: number, y: number, z: number) => Promise<ClientOscMessage>;
    reset: () => Promise<ClientOscMessage>;
};
