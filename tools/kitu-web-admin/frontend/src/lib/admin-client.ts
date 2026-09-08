import { env } from "$env/dynamic/public";
import type { AdminClientOptions } from "@kitu/admin/client";

export const adminClientOptions: AdminClientOptions = {
  apiUrl: env.PUBLIC_KITU_ADMIN_API_URL ?? "http://localhost:8787",
  webSocketUrl: env.PUBLIC_KITU_ADMIN_WS_URL ?? "ws://localhost:8787/ws",
  webTransportUrl: env.PUBLIC_KITU_ADMIN_WT_URL,
  webTransportCertificateSha256: env.PUBLIC_KITU_ADMIN_WT_CERT_SHA256,
  kepRoute: env.PUBLIC_KITU_ADMIN_KEP_ROUTE ?? "/room/main",
};

export const apiBaseUrl = () => adminClientOptions.apiUrl;
