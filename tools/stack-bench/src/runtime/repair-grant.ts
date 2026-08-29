import { existsSync } from 'node:fs';
import { join, relative, resolve, sep } from 'node:path';

import { currentEngineIdentity, readArtifact } from '../evidence/artifacts.js';
import type { Artifact } from '../evidence/artifacts.js';
import { canonicalDefinitionJson } from '../composition/definition-plan.mjs';
import { hashDirectory } from '../evidence/provenance.js';

export interface RepairOutcome extends Record<string, unknown> {
  kind: string;
  appFailures?: string[];
  inconclusive?: unknown[];
  harnessFailures?: unknown[];
}

export interface RepairBudget extends Record<string, unknown> {
  status: string;
  budgetRounds: number;
  roundsUsed: number;
  stopReason?: string | null;
}

export interface RepairSelection extends Record<string, unknown> {
  sha256?: string;
  scoredPoints?: number;
  recipe?: { id?: string; version?: string };
}

interface CheckpointReference extends Record<string, unknown> {
  artifact: string;
  directory: string;
  sha256: string;
  files: number;
}

export interface RepairLevel extends Record<string, unknown> {
  level: number;
  outcome: RepairOutcome;
  repair: RepairBudget;
  fixRounds: number;
  score: number;
  max: number;
  selection?: RepairSelection;
  checkpoint?: CheckpointReference;
  buildCostUsd?: number;
  resumeCostUsd?: number;
  fixCostUsd?: number;
  durationSec?: number;
}

interface RepairContinuation extends Record<string, unknown> {
  rootRunId: string;
  grantIndex: number;
  cumulativeRoundsAfter: number;
  downstreamLevelsToRerun: number[];
  cumulativeCostAfterUsd: number;
  cumulativeDurationAfterSec: number;
}

interface RepairCondition extends Record<string, unknown> {
  guidance?: { documents?: Record<string, unknown> };
}

interface RepairParentPayload extends Record<string, unknown> {
  mode?: { id?: string };
  progressionStatus?: unknown;
  levels: RepairLevel[];
  contaminated?: boolean;
  continuation?: RepairContinuation;
  track: string;
  backend: string;
  model: string;
  guidance: unknown;
  condition?: RepairCondition | null;
  selectionRequest?: unknown;
  skills?: unknown[];
  runtime?: { buildImage?: string | null; url?: string | null };
  backendLease?: { runIndex?: number };
}

export interface RepairParent extends RepairParentPayload {
  id: string;
  kind: string;
  artifactEnvelope: {
    attempt: Artifact['attempt'];
    timestamps: unknown;
    identities: Artifact['identities'];
  };
}

interface SourceCheckpointPayload extends Record<string, unknown> {
  source: { directory: string; sha256: string; files: number };
  track: string;
  backend: string;
  level: number;
  repair: unknown;
  outcome: unknown;
  selectionSha256: string | null;
}

export interface RepairConfiguration {
  backend: string;
  track: string;
  level: number;
  recipe: string | null;
  runIndex: number;
  agentAdapter: string;
  model: string;
  guidance: unknown;
  guidanceDocument: unknown;
  condition: RepairCondition | null;
  selectionRequest: unknown;
  skills: unknown[];
  buildImage: string | null;
  url: string | null;
}

export interface InspectedRepairParent {
  root: string;
  parent: RepairParent;
  parentArtifact: Artifact<RepairParentPayload>;
  level: RepairLevel;
  checkpoint: Artifact<SourceCheckpointPayload>;
  sourcePath: string;
  rootRunId: string;
  grantIndex: number;
  cumulativeRoundsBefore: number;
  downstreamLevelsToRerun: number[];
  cumulativeCostBeforeUsd: number;
  cumulativeDurationBeforeSec: number;
  configuration: RepairConfiguration;
}

export interface RepairGrant extends Record<string, unknown> {
  schemaVersion: 1;
  rootRunId: string;
  parentRunId: string;
  level: number;
  grantIndex: number;
  roundsGranted: number;
  cumulativeRoundsBefore: number;
  cumulativeRoundsAfter: number;
  parentCheckpointSha256: string;
  downstreamLevelsToRerun: number[];
  cumulativeCostBeforeUsd: number;
  cumulativeCostAfterUsd: number;
  cumulativeDurationBeforeSec: number;
  cumulativeDurationAfterSec: number;
  baseline: null;
  resumeSetup: null;
}

export interface RepairGrantResolution extends InspectedRepairParent {
  grant: RepairGrant;
}

export interface RepairBaselineInput {
  score: number;
  max: number;
  selectionSha256: string | null;
  sourceSha256: string;
  expectedSourceSha256: string;
  outcome: RepairOutcome;
}

export interface RepairBaselineParent {
  score?: number;
  max?: number;
  selection?: { sha256?: string | null };
  outcome?: RepairOutcome;
}

export interface RepairBaselineComparison {
  reproduced: boolean;
  mismatches: string[];
}

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function same(left: unknown, right: unknown): boolean {
  return canonicalDefinitionJson(left) === canonicalDefinitionJson(right);
}

function child(root: string, relativePath: unknown, label: string): string {
  if (typeof relativePath !== 'string' || !relativePath) throw new Error(`${label} is missing`);
  const absoluteRoot = resolve(root);
  const absolute = resolve(absoluteRoot, relativePath);
  const rel = relative(absoluteRoot, absolute);
  if (rel === '..' || rel.startsWith(`..${sep}`) || rel === '') {
    throw new Error(`${label} is not a child of the parent result`);
  }
  return absolute;
}

function requireCompletedFailure(level: RepairLevel | undefined,
  parent: RepairParent): asserts level is RepairLevel {
  if (!object(level)) throw new Error('parent run does not contain the requested level');
  if (level.outcome?.kind !== 'app_failure') {
    throw new Error(`L${level.level} is ${level.outcome?.kind ?? 'ungraded'}, not a conclusive application failure`);
  }
  if ((level.outcome.inconclusive?.length ?? 0) > 0
    || (level.outcome.harnessFailures?.length ?? 0) > 0) {
    throw new Error(`L${level.level} does not have complete conclusive measurement`);
  }
  const exhausted = level.repair?.status === 'budget-exhausted'
    && level.repair?.roundsUsed === level.repair?.budgetRounds;
  const paused = level.repair?.status === 'incomplete'
    && level.repair?.stopReason === 'repeated-findings'
    && level.repair?.roundsUsed > 0
    && level.repair?.roundsUsed < level.repair?.budgetRounds;
  if ((!exhausted && !paused)
    || !Number.isSafeInteger(level.repair?.budgetRounds)
    || level.repair.budgetRounds < 0
    || level.fixRounds !== level.repair.roundsUsed) {
    throw new Error(`L${level.level} did not exhaust or pause one coherent repair budget`);
  }
  if (!Number.isSafeInteger(level.score) || !Number.isSafeInteger(level.max)
    || level.max < 1 || level.score < 0 || level.score >= level.max) {
    throw new Error(`L${level.level} does not contain a measurable failing score`);
  }
  if (level.selection?.scoredPoints !== undefined && level.selection.scoredPoints !== level.max) {
    throw new Error(`L${level.level} score denominator does not match its selection`);
  }
  if (parent.contaminated === true) throw new Error('contaminated runs cannot receive repair grants');
}

function continuationFor(parent: RepairParent): RepairContinuation {
  if (!parent.continuation) throw new Error('repair continuation metadata is missing');
  return parent.continuation;
}

export function inspectRepairParent(parentDirectory: string,
  levelNumber: number): InspectedRepairParent {
  const root = resolve(parentDirectory);
  if (!Number.isSafeInteger(levelNumber) || levelNumber < 1) {
    throw new Error('repair grant level must be a positive integer');
  }
  const runPath = join(root, 'run.json');
  if (!existsSync(runPath)) throw new Error(`parent run does not exist: ${runPath}`);
  const parentArtifact = readArtifact<RepairParentPayload>(runPath);
  if (!['benchmark_run', 'repair_continuation'].includes(parentArtifact.kind)) {
    throw new Error(`parent result is ${parentArtifact.kind}, not a benchmark run or repair continuation`);
  }
  if (parentArtifact.timestamps.completedAt === null) throw new Error('parent result is not complete');
  const parentEngine = parentArtifact.identities.engine;
  if (parentEngine?.sha256 !== currentEngineIdentity().sha256) {
    throw new Error('parent result uses a different harness executable');
  }
  const parent: RepairParent = { ...parentArtifact.payload,
    id: parentArtifact.id, kind: parentArtifact.kind,
    artifactEnvelope: { attempt: parentArtifact.attempt, timestamps: parentArtifact.timestamps,
      identities: parentArtifact.identities } };
  if (parent.mode?.id === 'dependency' || parent.progressionStatus !== undefined) {
    throw new Error('generic repair grants do not support dependency campaigns');
  }
  const level = parent.levels?.find(item => item.level === levelNumber);
  requireCompletedFailure(level, parent);

  const checkpointRef = level.checkpoint;
  if (!object(checkpointRef)) throw new Error(`L${levelNumber} has no source checkpoint`);
  const checkpointPath = child(root, checkpointRef.artifact, 'checkpoint artifact');
  const sourcePath = child(root, checkpointRef.directory, 'checkpoint source');
  if (!existsSync(sourcePath)) throw new Error(`checkpoint source does not exist: ${sourcePath}`);
  const checkpoint = readArtifact<SourceCheckpointPayload>(checkpointPath, {
    expectedKind: 'source_checkpoint', expectedId: `${parent.id}-l${levelNumber}-checkpoint`,
  });
  if (checkpoint.attempt.parentId !== parent.id) throw new Error('checkpoint parent does not match the run');
  const checkpointEngine = checkpoint.identities.engine;
  if (checkpointEngine?.sha256 !== parentEngine.sha256) {
    throw new Error('checkpoint and parent harness identities differ');
  }
  const expectedSource = { directory: checkpointRef.directory, sha256: checkpointRef.sha256,
    files: checkpointRef.files };
  if (!same(checkpoint.payload.source, expectedSource)) throw new Error('checkpoint source reference changed');
  if (checkpoint.payload.track !== parent.track || checkpoint.payload.backend !== parent.backend
    || checkpoint.payload.level !== levelNumber) throw new Error('checkpoint scope does not match the parent run');
  if (!same(checkpoint.payload.repair, level.repair)
    || !same(checkpoint.payload.outcome, level.outcome)
    || checkpoint.payload.selectionSha256 !== (level.selection?.sha256 ?? null)) {
    throw new Error('checkpoint does not bind the parent level result');
  }
  const source = hashDirectory(sourcePath);
  if (source.sha256 !== checkpointRef.sha256 || source.files.length !== checkpointRef.files) {
    throw new Error('checkpoint source bytes do not match their recorded identity');
  }

  const continuation = parent.kind === 'repair_continuation' ? continuationFor(parent) : null;
  const rootRunId = continuation ? continuation.rootRunId : parent.id;
  const grantIndex = continuation ? continuation.grantIndex + 1 : 1;
  const cumulativeRoundsBefore = continuation
    ? continuation.cumulativeRoundsAfter : level.repair.roundsUsed;
  const downstreamLevelsToRerun = continuation
    ? continuation.downstreamLevelsToRerun
    : parent.levels.map(item => item.level).filter(item => item > levelNumber).sort((a, b) => a - b);
  const prerequisiteLevels = parent.levels.filter(item => item.level <= levelNumber);
  const cumulativeCostBeforeUsd = continuation
    ? continuation.cumulativeCostAfterUsd
    : prerequisiteLevels.reduce((total, item) => total
      + (item.buildCostUsd ?? item.resumeCostUsd ?? 0) + (item.fixCostUsd ?? 0), 0);
  const cumulativeDurationBeforeSec = continuation
    ? continuation.cumulativeDurationAfterSec
    : prerequisiteLevels.reduce((total, item) => total + (item.durationSec ?? Number.NaN), 0);
  if (!Number.isFinite(cumulativeCostBeforeUsd) || cumulativeCostBeforeUsd < 0
    || !Number.isFinite(cumulativeDurationBeforeSec) || cumulativeDurationBeforeSec < 0) {
    throw new Error('parent run does not contain cumulative cost and duration');
  }
  const guidanceDocument = parent.condition?.guidance?.documents?.[parent.backend] ?? null;
  const runIndex = parent.backendLease?.runIndex;
  if (typeof runIndex !== 'number' || !Number.isSafeInteger(runIndex) || runIndex < 0) {
    throw new Error('parent run does not identify its resource slot');
  }
  const agentAdapter = parentArtifact.identities.agentAdapter?.id;
  if (typeof agentAdapter !== 'string' || !agentAdapter) {
    throw new Error('parent run does not identify its agent adapter');
  }
  const recipe = object(level.selection?.recipe)
    ? `${level.selection.recipe.id}@${level.selection.recipe.version}` : null;
  return {
    root,
    parent,
    parentArtifact,
    level,
    checkpoint,
    sourcePath,
    rootRunId,
    grantIndex,
    cumulativeRoundsBefore,
    downstreamLevelsToRerun,
    cumulativeCostBeforeUsd,
    cumulativeDurationBeforeSec,
    configuration: {
      backend: parent.backend,
      track: parent.track,
      level: levelNumber,
      recipe,
      runIndex,
      agentAdapter,
      model: parent.model,
      guidance: parent.guidance,
      guidanceDocument,
      condition: parent.condition ?? null,
      selectionRequest: parent.selectionRequest ?? { packs: [], checks: [] },
      skills: parent.skills ?? [],
      buildImage: parent.runtime?.buildImage ?? null,
      url: parent.runtime?.url ?? null,
    },
  };
}

export function createRepairGrant(parentDirectory: string,
  { level, rounds }: { level: number; rounds: number }): RepairGrantResolution {
  if (!Number.isSafeInteger(rounds) || rounds < 1 || rounds > 20) {
    throw new Error('repair grant rounds must be an integer from 1 through 20');
  }
  const inspected = inspectRepairParent(parentDirectory, level);
  return {
    ...inspected,
    grant: {
      schemaVersion: 1,
      rootRunId: inspected.rootRunId,
      parentRunId: inspected.parent.id,
      level,
      grantIndex: inspected.grantIndex,
      roundsGranted: rounds,
      cumulativeRoundsBefore: inspected.cumulativeRoundsBefore,
      cumulativeRoundsAfter: inspected.cumulativeRoundsBefore,
      parentCheckpointSha256: inspected.checkpoint.payload.source.sha256,
      downstreamLevelsToRerun: inspected.downstreamLevelsToRerun,
      cumulativeCostBeforeUsd: inspected.cumulativeCostBeforeUsd,
      cumulativeCostAfterUsd: inspected.cumulativeCostBeforeUsd,
      cumulativeDurationBeforeSec: inspected.cumulativeDurationBeforeSec,
      cumulativeDurationAfterSec: inspected.cumulativeDurationBeforeSec,
      baseline: null,
      resumeSetup: null,
    },
  };
}

export function compareRepairBaseline(parentLevel: RepairBaselineParent | null | undefined,
  { score, max, selectionSha256, sourceSha256, expectedSourceSha256, outcome }:
  RepairBaselineInput): RepairBaselineComparison {
  const expectedFailures = [...(parentLevel?.outcome?.appFailures ?? [])].sort();
  const actualFailures = [...(outcome?.appFailures ?? [])].sort();
  const mismatches = [];
  if (score !== parentLevel?.score) mismatches.push('score');
  if (max !== parentLevel?.max) mismatches.push('denominator');
  if (selectionSha256 !== (parentLevel?.selection?.sha256 ?? null)) mismatches.push('selection');
  if (sourceSha256 !== expectedSourceSha256) mismatches.push('source');
  if (outcome?.kind !== 'app_failure') mismatches.push('outcome');
  if ((outcome?.inconclusive?.length ?? 0) > 0 || (outcome?.harnessFailures?.length ?? 0) > 0) {
    mismatches.push('measurement');
  }
  if (!same(actualFailures, expectedFailures)) mismatches.push('failed criteria');
  return { reproduced: mismatches.length === 0, mismatches };
}
