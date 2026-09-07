import { apiBaseUrl } from "./admin-client";

export type TimelineClipId = "boss-telegraph" | "floor-transition";

export type TimelineVersion = {
  hash: string;
  contractVersion: 1;
  tickRate: 60;
  clips: Array<{
    id: TimelineClipId;
    sourceSha256: string;
    bytes: number[];
  }>;
};

type CuePosition = {
  id: string;
  clipId: TimelineClipId;
  startedTick: number;
  offsetTick: number;
  nextEventIndex: number;
  eventCount: number;
};

export type BossCue = CuePosition & {
  entityId: number;
  floor: number;
  radius: number;
  intensity: number;
};

export type FloorCue = CuePosition & {
  fromFloor: number;
  toFloor: number;
  opacity: number;
};

export type PresentationSnapshot = {
  contractVersion: 1;
  run: number;
  tick: number;
  simulationStep: number;
  bosses: BossCue[];
  floor: FloorCue | null;
};

export type TimelineSnapshot = {
  run: number;
  active: TimelineVersion | null;
  pending: TimelineVersion;
  presentation: PresentationSnapshot;
};

export type TimelineStatus = {
  path: string | null;
  readOnly: boolean;
  runtime: TimelineSnapshot;
  candidate: TimelineVersion | null;
  diagnostics: string[];
};

async function request<T>(path: string, body?: object): Promise<T> {
  const response = await fetch(`${apiBaseUrl()}/arena/timeline${path}`, {
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

export const inspectTimeline = () => request<TimelineStatus>("");
export const validateTimeline = () => request<TimelineStatus>("/validate", {});
export const stageTimeline = (candidate: TimelineVersion) =>
  request<{ sequence: number; hash: string }>("/stage", {
    hash: candidate.hash,
  });
