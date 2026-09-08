import { getContext, setContext } from 'svelte'
import type { AdminClient } from './create-admin-client.js'

const ADMIN_CLIENT = Symbol.for('@kitu/admin/client')

export function setAdminClientContext(client: AdminClient) {
  setContext(ADMIN_CLIENT, client)
  return client
}

export function getAdminClient() {
  const client = getContext<AdminClient | undefined>(ADMIN_CLIENT)
  if (!client) throw new Error('@kitu/admin client context is missing')
  return client
}
