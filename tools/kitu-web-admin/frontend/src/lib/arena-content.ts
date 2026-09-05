import { apiBaseUrl } from "./admin-client";

export type ArenaConfig = {
  schemaVersion: number;
  items: Array<{
    id: string;
    name: string;
    kind: number;
    damage: number;
    bossDamage: number;
    interval: number;
  }>;
  enemies: Array<{
    kind: number;
    health: number;
    damage: number;
    speed: number;
    range: number;
    interval: number;
    radius: number;
  }>;
  difficulty: { healthGrowth: number; damageGrowth: number };
  chests: Array<{ phase: "preparing" | "boss"; itemId: string }>;
};

export type ContentVersion = {
  hash: string;
  sourceSha256: string;
  tanuRevision: string;
  values: ArenaConfig;
};

export type ContentStatus = {
  path: string;
  runtime: {
    run: number;
    active: ContentVersion | null;
    pending: ContentVersion;
  };
  candidate: ContentVersion | null;
  diagnostics: string[];
  savedRun: number | null;
  persistenceError: string | null;
};

async function request<T>(path: string, body?: object): Promise<T> {
  const response = await fetch(`${apiBaseUrl()}/arena/content${path}`, {
    method: body ? "POST" : "GET",
    ...(body
      ? {
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(body),
        }
      : {}),
  });
  const result = await response.json();
  if (!response.ok) throw new Error(result.error ?? response.statusText);
  return result as T;
}

export const inspectContent = () => request<ContentStatus>("");
export const validateContent = () => request<ContentStatus>("/validate", {});
export const stageContent = (candidate: ContentVersion) =>
  request<{ sequence: number; hash: string }>("/stage", {
    hash: candidate.hash,
    sourceSha256: candidate.sourceSha256,
  });
