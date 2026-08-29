import type { CostRun } from './cost-proof.js';

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
export function createArtifact<TPayload = Record<string, unknown>>(input: {
  kind: string;
  id: string;
  attempt?: { id: string; parentId?: string | null } | null;
  timestamps?: { startedAt?: string; completedAt?: string | null } | null;
  identities?: unknown;
  payload?: TPayload;
}): Artifact<TPayload>;
export function validateArtifact<TPayload = Record<string, unknown>>(input: unknown,
  options?: { source?: string }): Artifact<TPayload>;
export function currentEngineIdentity(): { sha256: string; [key: string]: unknown };
export function readArtifact<TPayload = Record<string, unknown>>(path: string,
  options?: { expectedId?: string | null; expectedKind?: string | null }): Artifact<TPayload>;
export function readArtifactPayload<TPayload = Record<string, unknown>>(path: string,
  options?: { expectedId?: string | null; expectedKind?: string | null }): TPayload;
export function readRunJson(path: string, expectedRunId?: string): CostRun;
export function writeArtifact(path: string, input: unknown): Artifact;
export function writeRunJson(path: string, input: unknown): Artifact;
