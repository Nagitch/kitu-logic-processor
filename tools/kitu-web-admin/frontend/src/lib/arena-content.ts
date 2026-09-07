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
  tanuRevision?: string;
  provenance?: {
    version: 1;
    sources: Array<{
      layer: ContentLayer;
      format: ContentFormat;
      schemaVersion: number;
      evaluator: string;
      sourceSha256: string;
    }>;
    origins: Record<string, ContentLayer>;
  };
  values: ArenaConfig;
};

export type ContentLayer = "base" | "difficulty" | "event" | "debug";
export type ContentFormat = "tmd" | "sqlite";
export type ContentDifference = {
  path: string;
  before: unknown;
  after: unknown;
  winningLayer: ContentLayer;
};

export type ContentStatus = {
  path: string;
  readOnly: boolean;
  sources: Array<{
    layer: ContentLayer;
    format: ContentFormat;
    path: string;
    sourceSha256: string;
    evaluator: string;
    schemaVersion: number;
  }>;
  differences: {
    active: ContentDifference[] | null;
    pending: ContentDifference[] | null;
  };
  origins: {
    candidate: Record<string, ContentLayer> | null;
    active: Record<string, ContentLayer> | null;
    pending: Record<string, ContentLayer>;
  };
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
