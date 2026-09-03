import { cpSync, existsSync, rmSync } from 'node:fs';
import { join } from 'node:path';

import { ARTIFACT_FILE, readArtifactPayload, writeArtifact } from '../evidence/artifacts.js';
import type { GradeBundlePayload } from '../evidence/benchmark-run.js';
import { classifyBundle } from '../evidence/outcomes.js';
import type { RunOutcome } from '../evidence/outcomes.js';
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

export function finalPackageEvidenceRequired(outcome: RunOutcome | null | undefined,
  levels: ReadonlyArray<{ graded?: boolean }>): boolean {
  return ['passed', 'app_failure'].includes(outcome?.kind ?? '')
    && levels.some(level => level.graded === true);
}

export function preserveFinalPackageEvidence(
  { appDir, outputDir }: { appDir: string; outputDir: string },
): {
  source: { directory: string; sha256: string; files: number };
  grading: { directory: string; artifact: string; sourceSha256: string };
} {
  const failures: string[] = [];
  let source: { directory: string; sha256: string; files: number } | null = null;
  let grading: { directory: string; artifact: string; sourceSha256: string } | null = null;
  const message = (error: unknown): string => error instanceof Error ? error.message : String(error);

  try {
    const live = hashAppSource(appDir);
    const sourceDirectory = join(outputDir, 'source');
    snapshotAppSource(appDir, sourceDirectory);
    const saved = hashDirectory(sourceDirectory);
    if (saved.sha256 !== live.sha256 || saved.files.length !== live.files.length) {
      throw new Error('preserved final source differs from the live application source');
    }
    source = { directory: 'source', sha256: saved.sha256, files: saved.files.length };
  } catch (error) {
    failures.push(`source: ${message(error).split(/\r?\n/)[0]}`);
  }

  try {
    const from = join(appDir, 'stack-bench');
    const gradingDirectory = join(outputDir, 'grading');
    if (!existsSync(join(from, ARTIFACT_FILE.gradeBundle))) {
      throw new Error(`final grader produced no ${ARTIFACT_FILE.gradeBundle}`);
    }
    rmSync(gradingDirectory, { recursive: true, force: true });
    cpSync(from, gradingDirectory, {
      recursive: true,
      filter: path => !/[\\/]media([\\/]|$)/.test(path),
    });
    const bundle = readArtifactPayload<GradeBundlePayload>(
      join(gradingDirectory, ARTIFACT_FILE.gradeBundle), {
      expectedKind: 'grade_bundle',
    });
    if (!source || bundle.source?.sha256 !== source.sha256) {
      throw new Error('final grading bundle does not match the preserved application source');
    }
    grading = { directory: 'grading', artifact: `grading/${ARTIFACT_FILE.gradeBundle}`,
      sourceSha256: bundle.source.sha256 };
  } catch (error) {
    failures.push(`grading: ${message(error).split(/\r?\n/)[0]}`);
  }
  if (failures.length) {
    throw new Error(`could not preserve mandatory result package evidence: ${failures.join('; ')}`);
  }
  if (!source || !grading) throw new Error('could not preserve mandatory result package evidence');
  return { source, grading };
}

export function sourceBoundFirstBuildOutcome(bundle: GradeBundlePayload | null,
  source: object | null): RunOutcome {
  if (source) return classifyBundle(bundle);
  const reason = 'the first-build source could not be preserved and verified';
  return { kind: 'harness_failure', phase: 'first-build-source', reason,
    appFailures: [], inconclusive: [], harnessFailures: [reason] };
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
      schemaVersion: 3,
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
