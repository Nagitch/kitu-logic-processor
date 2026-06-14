export type WorldObject = {
  id: string;
  kind: string;
  x: number;
  y: number;
  z: number;
  color: string;
};

export type DebugLogEntry = {
  id: number;
  level: "info" | "warn" | "error";
  message: string;
  oscAddress?: string | null;
  tick: number;
};

export type WorldSnapshot = {
  tick: number;
  objects: WorldObject[];
};

export type JsonOscArg =
  | { type: "int"; value: number }
  | { type: "int64"; value: number }
  | { type: "float"; value: number }
  | { type: "str"; value: string }
  | { type: "bool"; value: boolean };

export type ClientOscMessage = {
  address: string;
  args: JsonOscArg[];
};

export type ServerEvent =
  | { type: "connected"; protocol: string; tick: number }
  | { type: "state"; snapshot: WorldSnapshot }
  | { type: "log"; entry: DebugLogEntry }
  | { type: "osc"; address: string; args: JsonOscArg[] }
  | { type: "error"; message: string };
