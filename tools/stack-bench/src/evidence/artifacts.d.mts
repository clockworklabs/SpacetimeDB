import type { CostRun } from './cost-proof.mjs';

export interface Artifact<TPayload = Record<string, unknown>> {
  id: string;
  kind: string;
  attempt: { parentId?: string | null; [key: string]: unknown };
  identities: {
    agentAdapter: unknown;
    stackAdapter: unknown;
    [key: string]: unknown;
  };
  payload: TPayload;
  [key: string]: unknown;
}

export function emptyArtifactIdentities(overrides?: Record<string, unknown>): unknown;
export function currentEngineIdentity(): { sha256: string; [key: string]: unknown };
export function readArtifact<TPayload = Record<string, unknown>>(path: string,
  options?: { expectedId?: string | null; expectedKind?: string | null }): Artifact<TPayload>;
export function readArtifactPayload<TPayload = Record<string, unknown>>(path: string,
  options?: { expectedId?: string | null; expectedKind?: string | null }): TPayload;
export function readRunJson(path: string, expectedRunId?: string): CostRun;
export function writeArtifact(path: string, input: unknown): Artifact;
export function writeRunJson(path: string, input: unknown): Artifact;
