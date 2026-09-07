import type { ClientOscMessage } from './types.js';
export declare function encodeOscPacket(message: ClientOscMessage): Uint8Array<ArrayBuffer>;
export declare function encodeKepStreamFrame(payload: Uint8Array, route: string): Uint8Array<ArrayBuffer>;
export declare function decodeKepJsonFrames(bytes: Uint8Array): string[];
