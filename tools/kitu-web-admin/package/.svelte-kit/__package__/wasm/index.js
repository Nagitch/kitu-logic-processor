export function createOscIrLoader(options = {}) {
    let modulePromise = null;
    const dynamicImport = options.importModule ?? importRuntimeModule;
    const documentBase = typeof document === 'undefined' ? 'http://localhost/' : document.baseURI;
    const baseUrl = options.baseUrl instanceof URL ? options.baseUrl.href : new URL(options.baseUrl ?? '.', documentBase).href;
    const resolve = (value) => (value instanceof URL ? value.href : new URL(value, baseUrl).href);
    const moduleUrl = resolve(options.moduleUrl ?? 'kitu-osc-ir-wasm/kitu_osc_ir_wasm.js');
    const wasmUrl = options.wasmUrl ? resolve(options.wasmUrl) : undefined;
    async function load() {
        modulePromise ??= dynamicImport(moduleUrl)
            .then(async (value) => {
            const wasm = value;
            await wasm.default(wasmUrl);
            return wasm;
        })
            .catch((error) => {
            modulePromise = null;
            throw new Error(`OSC-IR WASM bindings are unavailable: ${error instanceof Error ? error.message : String(error)}`);
        });
        return modulePromise;
    }
    return {
        initialize: async () => {
            await load();
        },
        spawn: async (kind, x, y, z) => validate((await load()).admin_world_spawn(kind, x, y, z)),
        move: async (id, x, y, z) => validate((await load()).admin_world_move(id, x, y, z)),
        reset: async () => validate((await load()).admin_world_reset())
    };
}
async function importRuntimeModule(url) {
    return import(/* @vite-ignore */ url);
}
function validate(value) {
    if (!record(value) || typeof value.address !== 'string' || !Array.isArray(value.args) || !value.args.every(isArg))
        throw new Error('OSC-IR WASM returned an invalid message');
    return { address: value.address, args: value.args };
}
function isArg(value) {
    if (!record(value) || typeof value.type !== 'string')
        return false;
    if (['int', 'int64', 'float'].includes(value.type))
        return typeof value.value === 'number';
    if (value.type === 'str')
        return typeof value.value === 'string';
    return value.type === 'bool' && typeof value.value === 'boolean';
}
function record(value) {
    return typeof value === 'object' && value !== null;
}
