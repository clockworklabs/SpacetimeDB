import { randomUUID } from 'node:crypto';
import { mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';

import type { CostRun } from './cost-proof.js';
import { recipeArtifactIdentities } from './artifact-identities.js';
import {
  ARTIFACT_SCHEMA_VERSION,
  createArtifact,
  validateArtifact,
  validateGradePayload,
} from './artifact-schema.js';
import type {
  Artifact,
  ArtifactKind,
  GradeArtifactPayload,
} from './artifact-schema.js';

type UnknownRecord = Record<string, unknown>;

export interface ArtifactReadOptions {
  expectedId?: string | null;
  expectedKind?: ArtifactKind | null;
}

const isObject = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function object(value: unknown, message: string): UnknownRecord {
  if (!isObject(value)) throw new Error(`invalid artifact: ${message}`);
  return value;
}

export function writeArtifact(path: string, input: unknown): Artifact {
  const candidate = object(input, `${path} must be an object`);
  if (candidate.artifactSchemaVersion !== undefined
    && candidate.artifactSchemaVersion !== ARTIFACT_SCHEMA_VERSION) {
    throw new Error(`invalid artifact: ${path} uses unsupported schema ${candidate.artifactSchemaVersion}`);
  }
  const artifact = candidate.artifactSchemaVersion === ARTIFACT_SCHEMA_VERSION
    ? validateArtifact(candidate, { source: path })
    : createArtifact({
      kind: candidate.kind,
      id: typeof candidate.id === 'string' ? candidate.id : '',
      attempt: candidate.attempt,
      timestamps: candidate.timestamps,
      identities: candidate.identities,
      payload: candidate.payload,
    });
  mkdirSync(dirname(path), { recursive: true });
  const temporary = `${path}.${process.pid}.${randomUUID()}.tmp`;
  try {
    writeFileSync(temporary, `${JSON.stringify(artifact, null, 2)}\n`, { flag: 'wx' });
    renameSync(temporary, path);
  } catch (error) {
    rmSync(temporary, { force: true });
    throw error;
  }
  return artifact;
}

export function readArtifact<TPayload extends object = UnknownRecord>(
  path: string,
  { expectedId = null, expectedKind = null }: ArtifactReadOptions = {},
): Artifact<TPayload> {
  let input: unknown;
  try {
    input = JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    throw new Error(`cannot read artifact ${path}: ${error instanceof Error ? error.message : String(error)}`,
      { cause: error });
  }
  const artifact = validateArtifact(input, { source: path });
  if (expectedId !== null && artifact.id !== expectedId) {
    throw new Error(`artifact ${path} belongs to ${artifact.id}, not ${expectedId}`);
  }
  if (expectedKind !== null && artifact.kind !== expectedKind) {
    throw new Error(`artifact ${path} is ${artifact.kind}, not ${expectedKind}`);
  }
  return artifact as Artifact<TPayload>;
}

export function artifactPayload<TPayload extends object>(artifact: Artifact<TPayload>): TPayload & UnknownRecord {
  return { ...artifact.payload, artifactSchemaVersion: artifact.artifactSchemaVersion,
    kind: artifact.kind, id: artifact.id, artifactEnvelope: {
      attempt: artifact.attempt,
      timestamps: artifact.timestamps,
      identities: artifact.identities,
    } };
}

export function readArtifactPayload<TPayload extends object = UnknownRecord>(
  path: string,
  options: ArtifactReadOptions = {},
): TPayload & UnknownRecord {
  return artifactPayload(readArtifact<TPayload>(path, options));
}

export function readGradeArtifactPayload(path: string): GradeArtifactPayload {
  return validateGradePayload(readArtifactPayload(path, { expectedKind: 'grade' }));
}

// Run producers use a flat in-memory record. Only the schema-v2 envelope reaches disk.
export function writeRunJson(path: string, run: unknown): Artifact {
  if (!isObject(run) || typeof run.id !== 'string' || !run.id) {
    throw new Error('run artifact requires a non-empty id');
  }
  if (run.artifactSchemaVersion !== undefined
    && run.artifactSchemaVersion !== ARTIFACT_SCHEMA_VERSION) {
    throw new Error(`run artifact schema ${run.artifactSchemaVersion} is not supported`);
  }
  const { id, kind = 'benchmark_run', startedAt, completedAt, generatedAt,
    identities, attempt, parentAttemptId, artifactSchemaVersion: _schema, ...payload } = run;
  return writeArtifact(path, {
    kind,
    id,
    attempt: attempt ?? { id, parentId: parentAttemptId ?? null },
    timestamps: { startedAt: startedAt ?? generatedAt ?? new Date().toISOString(),
      completedAt: completedAt ?? null },
    identities: identities ?? recipeArtifactIdentities(payload.recipeRelease, {
      stackAdapter: payload.backend ? { id: payload.backend } : null,
    }),
    payload,
  });
}

export function readRunJson(path: string, expectedRunId?: string): CostRun & UnknownRecord {
  return readArtifactPayload(path, { expectedId: expectedRunId }) as CostRun & UnknownRecord;
}
