import { getContext, setContext } from 'svelte';
const ADMIN_CLIENT = Symbol.for('@kitu/admin/client');
export function setAdminClientContext(client) {
    setContext(ADMIN_CLIENT, client);
    return client;
}
export function getAdminClient() {
    const client = getContext(ADMIN_CLIENT);
    if (!client)
        throw new Error('@kitu/admin client context is missing');
    return client;
}
