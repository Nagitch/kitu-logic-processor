import { apiBaseUrl } from "./admin-client";

export type ScriptDiagnostic = {
  kind: string;
  message: string;
  line: number | null;
  column: number | null;
};

export type ScriptVersion = {
  hash: string;
  sourceSha256: string;
  source: string;
  contractVersion: number;
  policyVersion: string;
  rhaiVersion: string;
};

export type ScriptStatus = {
  path: string | null;
  readOnly: boolean;
  runtime: {
    run: number;
    active: ScriptVersion | null;
    pending: ScriptVersion;
    fault: {
      tick: number;
      enemyId: number;
      scriptHash: string;
      diagnostic: ScriptDiagnostic;
    } | null;
  };
  candidate: ScriptVersion | null;
  diagnostics: ScriptDiagnostic[];
};

async function request<T>(path: string, body?: object): Promise<T> {
  const response = await fetch(`${apiBaseUrl()}/arena/script${path}`, {
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

export const inspectScript = () => request<ScriptStatus>("");
export const validateScript = () => request<ScriptStatus>("/validate", {});
export const stageScript = (candidate: ScriptVersion) =>
  request<{ sequence: number; hash: string }>("/stage", {
    hash: candidate.hash,
  });
