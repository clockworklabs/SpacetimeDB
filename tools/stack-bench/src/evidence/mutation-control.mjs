import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { agentRecipeIdentity } from '../agents/agent-adapter-contract.mjs';
import { portsFor } from '../composition/tracks.mjs';
import { STACK_BENCH_ROOT } from '../project-paths.mjs';

const COMMAND_TIMEOUT_MS = 20 * 60_000;
const MUTATION_BASE_TIMEOUT_MS = 5 * 60_000;
const MUTATION_PROBE_TIMEOUT_MS = 75_000;

function restartSpecFor(args, appDir, track) {
  const port = portsFor(track, args.backend, args.runIndex).express ?? null;
  return { backend: args.backend, app: appDir, port: port == null ? null : Number(port),
    probe: track.restartProbe };
}

export function mutationControlArgv(args, appDir, url, track) {
  const output = join(args.out, 'mutation-control.json');
  const manifest = JSON.parse(readFileSync(args.mutations, 'utf8'));
  const recipeTask = args.recipeTasks?.get(Number(manifest.level))?.agentRequest
    ?? args.recipeTasks?.get(Number(manifest.level))?.request ?? null;
  const recipe = agentRecipeIdentity(args.recipe, recipeTask);
  return [join(STACK_BENCH_ROOT, 'grader', 'mutation-test.mjs'), '--app', appDir,
    '--url', url, '--mutations', args.mutations, '--backend', args.backend,
    '--track', args.track, '--run-index', String(args.runIndex), '--out', output,
    '--restart-spec', JSON.stringify(restartSpecFor(args, appDir, track)),
    '--parent-attempt-id', args.parentAttemptId,
    ...(recipe ? ['--recipe', recipe] : [])];
}

export function mutationControlTimeoutMs(manifest) {
  const mutations = Array.isArray(manifest?.mutations) ? manifest.mutations : [];
  const measured = MUTATION_BASE_TIMEOUT_MS + mutations.reduce((total, mutation) =>
    total + MUTATION_PROBE_TIMEOUT_MS + 2 * Number(mutation.settleMs ?? 4000), 0);
  return Math.max(COMMAND_TIMEOUT_MS, measured);
}
