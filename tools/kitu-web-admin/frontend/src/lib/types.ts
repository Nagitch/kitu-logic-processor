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

export type ActionValue =
  | { type: "string"; value: string }
  | { type: "float"; value: number }
  | { type: "int"; value: number }
  | { type: "bool"; value: boolean };

export type AppActionScope =
  | { type: "kitu-general" }
  | { type: "project"; appId: string };

export type ActionInputSpec = {
  name: string;
  label: string;
  valueType: "string" | "float" | "int" | "bool";
  required: boolean;
  default?: ActionValue | null;
};

export type AppActionDefinition = {
  id: string;
  scope: AppActionScope;
  label: string;
  description?: string | null;
  cli: { command: string };
  ui: { kind: "form" | "button"; submitLabel: string; destructive: boolean };
  inputs: ActionInputSpec[];
  output: { address: string; args: unknown[] };
};

export type AppActionCatalog = {
  actions: AppActionDefinition[];
};

export type ActionRunResponse = {
  actionId: string;
  osc: ClientOscMessage;
  snapshot: WorldSnapshot;
};

export type ServerEvent =
  | { type: "connected"; protocol: string; tick: number }
  | { type: "state"; snapshot: WorldSnapshot }
  | { type: "log"; entry: DebugLogEntry }
  | { type: "osc"; address: string; args: JsonOscArg[] }
  | { type: "error"; message: string };
