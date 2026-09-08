import { env } from '$env/dynamic/public'
import type { AdminClientOptions } from '@kitu/admin/client'

export const adminOptions: AdminClientOptions = {
  apiUrl: env.PUBLIC_KITU_ADMIN_API_URL ?? 'http://localhost:8787',
  webSocketUrl: env.PUBLIC_KITU_ADMIN_WS_URL ?? 'ws://localhost:8787/ws',
  webTransportUrl: env.PUBLIC_KITU_ADMIN_WT_URL,
  kepRoute: env.PUBLIC_KITU_ADMIN_KEP_ROUTE ?? '/room/main'
}
