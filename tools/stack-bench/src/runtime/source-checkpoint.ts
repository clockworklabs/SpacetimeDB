import { join } from 'node:path';

import { writeArtifact } from '../evidence/artifacts.js';
import { hashDirectory } from '../evidence/provenance.js';
import { hashAppSource, snapshotAppSource } from './source-snapshot.js';

const HASH = /^[a-f0-9]{64}$/;

export interface LevelCheckpoint {
  artifact: string;
  directory: string;
  sha256: string;
  files: number;
}

export interface PreserveLevelCheckpointOptions {
  appDir: string;
  outputDir: string;
  runId: string;
  identities?: unknown;
  track: string;
  backend: string;
  level: number;
  repair: unknown;
  outcome: unknown;
  selectionSha256?: string | null;
}

export function preserveLevelCheckpoint({ appDir, outputDir, runId, identities,
  track, backend, level, repair, outcome, selectionSha256 = null }:
  PreserveLevelCheckpointOptions): LevelCheckpoint {
  if (typeof runId !== 'string' || !runId) throw new Error('source checkpoint requires a run id');
  if (!Number.isSafeInteger(level) || level < 1) throw new Error('source checkpoint level is invalid');
  if (selectionSha256 !== null && !HASH.test(selectionSha256)) {
    throw new Error('source checkpoint selection identity is invalid');
  }

  const directory = `level-l${level}-source`;
  const artifact = `level-l${level}-checkpoint.json`;
  const sourcePath = join(outputDir, directory);
  const live = hashAppSource(appDir);
  snapshotAppSource(appDir, sourcePath);
  const saved = hashDirectory(sourcePath);
  if (live.sha256 !== saved.sha256 || live.files.length !== saved.files.length) {
    throw new Error('preserved level source differs from the live application source');
  }

  const source = { directory, sha256: saved.sha256, files: saved.files.length };
  const now = new Date().toISOString();
  writeArtifact(join(outputDir, artifact), {
    kind: 'source_checkpoint',
    id: `${runId}-l${level}-checkpoint`,
    attempt: { id: `${runId}-l${level}-checkpoint`, parentId: runId },
    timestamps: { startedAt: now, completedAt: now },
    identities,
    payload: {
      schemaVersion: 2,
      track,
      backend,
      level,
      source,
      repair,
      outcome,
      selectionSha256,
    },
  });
  return { artifact, ...source };
}
