import { join } from 'node:path';

import { agentRecipeIdentity } from '../agents/agent-adapter-contract.mjs';
import { portsFor } from '../composition/tracks.mjs';
import { STACK_BENCH_ROOT } from '../project-paths.mjs';

const COMMAND_TIMEOUT_MS = 20 * 60_000;

function restartSpecFor(args, appDir, track) {
  const port = portsFor(track, args.backend, args.runIndex).express ?? null;
  return { backend: args.backend, app: appDir, port: port == null ? null : Number(port),
    probe: track.restartProbe };
}

export function mutationControlArgv(args, appDir, url, track) {
  const level = args.levelList?.at(-1) ?? args.level;
  if (!Number.isSafeInteger(level) || level < 1) {
    throw new Error('mutation control requires a positive integer run level');
  }
  const output = join(args.out, 'mutation-control.json');
  const recipeTask = args.recipeTasks?.get(level)?.agentRequest
    ?? args.recipeTasks?.get(level)?.request ?? null;
  const recipe = agentRecipeIdentity(args.recipe, recipeTask);
  return [join(STACK_BENCH_ROOT, 'grader', 'mutation-test.mjs'), '--app', appDir,
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
    ...(args.mutationMaxRuntimeMinutes ? [
      '--max-runtime-minutes', String(args.mutationMaxRuntimeMinutes),
    ] : []),
    ...(args.mutationImageId ? ['--image-id', args.mutationImageId] : []),
    ...(recipe ? ['--recipe', recipe] : [])];
}

export function mutationControlTimeoutMs(manifest, maxRuntimeMinutes = 60) {
  if (!Array.isArray(manifest?.mutations)) {
    throw new Error('mutation control timeout requires a mutation manifest');
  }
  const boundedBatch = (Number(maxRuntimeMinutes) + 20) * 60_000;
  return Math.max(COMMAND_TIMEOUT_MS, boundedBatch);
}
