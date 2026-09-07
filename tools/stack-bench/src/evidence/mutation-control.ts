import { existsSync } from 'node:fs';
import { join } from 'node:path';

import { agentRecipeIdentity } from '../agents/agent-adapter-contract.js';
import { portsFor } from '../composition/tracks.js';
import type { Track } from '../composition/tracks.js';
import { compiledEntrypoint } from '../package-root.js';
import { ARTIFACT_FILE } from './artifacts.js';

interface SelectedCheck {
  stableKey?: unknown;
}

interface MutationRecipeTask {
  agentRequest?: unknown;
  request?: unknown;
  selection?: {
    scoredChecks?: Array<string | SelectedCheck>;
    checks?: Array<string | SelectedCheck>;
  };
}

export interface MutationControlArgs {
  levelList?: number[];
  level?: number;
  out: string;
  recipeTasks?: Map<number, MutationRecipeTask>;
  recipe?: string | null;
  mutations: string;
  backend: string;
  track: string;
  runIndex: number;
  parentAttemptId: string;
  mutationShardIndex?: number;
  mutationShardCount?: number;
  mutationResumeFrom?: string;
  mutationCheckpointOut?: string;
  mutationBaselineBundle?: string;
  expectedMutationCalibration?: unknown;
  mutationMaxRuntimeMinutes?: number;
  mutationImageId?: string;
}

interface MutationBaselineArgs {
  out?: string;
  levelList?: number[];
  referenceMutationOnly?: boolean;
  mutationBaselineBundle?: string;
}

export function pristineMutationBaselinePath(
  args: MutationBaselineArgs,
  exists: (path: string) => boolean = existsSync,
): string | null {
  if (args.referenceMutationOnly) return args.mutationBaselineBundle ?? null;
  if (args.mutationBaselineBundle) return args.mutationBaselineBundle;
  const level = args.levelList?.at(-1);
  if (typeof level !== 'number' || !Number.isSafeInteger(level) || level < 1 || !args.out) return null;
  const candidate = join(args.out, `first-build-l${level}-grading`, ARTIFACT_FILE.gradeBundle);
  return exists(candidate) ? candidate : null;
}

const COMMAND_TIMEOUT_MS = 20 * 60_000;
export const MUTATION_GRADE_MAX_TIMEOUT_MS = 15 * 60_000;

export function mutationGradeTimeoutMs(deadlineMs: number, nowMs: number = Date.now()): number {
  if (!Number.isFinite(deadlineMs) || !Number.isFinite(nowMs)) {
    throw new Error('mutation grade deadline must be finite');
  }
  const remainingMs = Math.floor(deadlineMs - nowMs);
  if (remainingMs <= 0) return 0;
  return Math.min(MUTATION_GRADE_MAX_TIMEOUT_MS, remainingMs);
}

function restartSpecFor(args: MutationControlArgs, appDir: string, track: Track): {
  backend: string;
  app: string;
  port: number | null;
  probe: string;
} {
  const port = portsFor(track, args.backend, args.runIndex).vite ?? null;
  return { backend: args.backend, app: appDir, port: port == null ? null : Number(port),
    probe: '' };
}

export function mutationControlArgv(
  args: MutationControlArgs,
  appDir: string,
  url: string,
  track: Track,
): string[] {
  const level = args.levelList?.at(-1) ?? args.level;
  if (typeof level !== 'number' || !Number.isSafeInteger(level) || level < 1) {
    throw new Error('mutation control requires a positive integer run level');
  }
  const output = join(args.out, ARTIFACT_FILE.mutationControl);
  const recipeTask = args.recipeTasks?.get(level)?.agentRequest
    ?? args.recipeTasks?.get(level)?.request ?? null;
  const recipe = agentRecipeIdentity(args.recipe, recipeTask);
  const selection = args.recipeTasks?.get(level)?.selection;
  const selectedCheckKeys = (selection?.scoredChecks ?? selection?.checks ?? [])
    .map(check => typeof check === 'string' ? check : check?.stableKey)
    .filter((stableKey): stableKey is string => typeof stableKey === 'string' && Boolean(stableKey));
  return [compiledEntrypoint('grader', 'mutation-test.js'), '--app', appDir,
    '--url', url, '--mutations', args.mutations, '--backend', args.backend,
    '--track', args.track, '--run-index', String(args.runIndex), '--out', output,
    '--level', String(level),
    '--restart-spec', JSON.stringify(restartSpecFor(args, appDir, track)),
    '--parent-attempt-id', args.parentAttemptId,
    ...(args.mutationShardCount === undefined ? [] : [
      '--mutation-shard-index', String(args.mutationShardIndex),
      '--mutation-shard-count', String(args.mutationShardCount),
    ]),
    ...(args.mutationResumeFrom ? ['--resume-from', args.mutationResumeFrom] : []),
    ...(args.mutationCheckpointOut ? ['--checkpoint-out', args.mutationCheckpointOut] : []),
    ...(args.mutationBaselineBundle ? ['--baseline-bundle', args.mutationBaselineBundle] : []),
    ...(args.expectedMutationCalibration ? [
      '--expected-calibration-json', JSON.stringify(args.expectedMutationCalibration),
    ] : []),
    ...(args.mutationMaxRuntimeMinutes ? [
      '--max-runtime-minutes', String(args.mutationMaxRuntimeMinutes),
    ] : []),
    ...(args.mutationImageId ? ['--image-id', args.mutationImageId] : []),
    ...selectedCheckKeys.flatMap(stableKey => ['--selected-check', stableKey]),
    ...(recipe ? ['--recipe', recipe] : [])];
}

export function mutationControlTimeoutMs(
  maxRuntimeMinutes: number = 60,
): number {
  if (!Number.isFinite(maxRuntimeMinutes) || maxRuntimeMinutes <= 0) {
    throw new Error('mutation control runtime limit must be a positive number');
  }
  const boundedBatch = (Number(maxRuntimeMinutes) + 20) * 60_000;
  return Math.max(COMMAND_TIMEOUT_MS, boundedBatch);
}
