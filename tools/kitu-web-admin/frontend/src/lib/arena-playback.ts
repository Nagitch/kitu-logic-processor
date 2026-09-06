import { apiBaseUrl } from "./admin-client";

export type PlaybackMode = {
  active: boolean;
  recordingId: string | null;
  tick: number;
  totalTicks: number;
  playing: boolean;
  seeking: boolean;
  error: string | null;
};
export type PlaybackStatus = {
  mode: PlaybackMode;
  state: {
    tick: number;
    floor: number;
    phase: number;
    elapsed: number;
    playerPosition: { x: number; y: number };
    inventory: { health: number; maxHealth: number };
    enemies: unknown[];
    [key: string]: unknown;
  };
  liveTick: number;
  contentHash: string;
  execution: { package: string; sourceHash: string; target: string };
};
export type RecordingFile = { id: string; bytes: number };
export type RecordingStatus = {
  runtimeId: string;
  ticks: number;
  liveTick: number;
  maxTicks: number;
  error: string | null;
};

async function request<T>(
  path: string,
  body?: object | ArrayBuffer,
): Promise<T> {
  const binary = body instanceof ArrayBuffer;
  const response = await fetch(`${apiBaseUrl()}/arena/${path}`, {
    method: body === undefined ? "GET" : "POST",
    ...(body === undefined
      ? {}
      : {
          headers: {
            "Content-Type": binary
              ? "application/octet-stream"
              : "application/json",
          },
          body: binary ? body : JSON.stringify(body),
        }),
  });
  const result = await response.json();
  if (!response.ok) throw new Error(result.error ?? response.statusText);
  return result as T;
}
export const inspectPlayback = () => request<PlaybackStatus>("playback");
export const inspectRecording = () => request<RecordingStatus>("recording");
export const listRecordings = () => request<RecordingFile[]>("recordings");
export const saveRecording = () => request<RecordingFile>("recording/save", {});
export const importRecording = (body: ArrayBuffer) =>
  request<RecordingFile>("recordings/import", body);
export const downloadRecording = (id: string) =>
  `${apiBaseUrl()}/arena/recordings/${encodeURIComponent(id)}`;
export const loadRecording = (id: string) =>
  request<PlaybackStatus>("playback/load", { id });
export const playbackCommand = (
  action: "play" | "pause" | "step" | "stop" | "live",
) => request<PlaybackStatus>("playback/command", { action });
export const seekPlayback = (tick: number) =>
  request<PlaybackStatus>("playback/seek", { tick });
