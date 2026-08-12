import { existsSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

import { readArtifactPayload } from './artifacts.mjs';
import { acquireCampaignLock, releaseCampaignLock } from './campaign-lock.mjs';
import { compileCampaignFile } from './campaign-compiler.mjs';
import { claimNextAttempt, finishCampaignExecution, initializeCampaignDirectory,
  markInterruptedExecution, readCampaignState, writeCampaignState } from './campaign-scheduler.mjs';
import { rescueSupervisedLease, runBounded } from './reference-live.mjs';
import { canonicalDefinitionJson } from './definition-plan.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const BENCH = join(ROOT, 'bench.mjs');

function contained(root, path, label) {
  const absoluteRoot = resolve(root);
  const absolute = resolve(absoluteRoot, path);
  const rel = relative(absoluteRoot, absolute);
  if (rel === '..' || rel.startsWith(`..${sep}`) || rel === '') {
    throw new Error(`${label} is not a child of the campaign directory`);
  }
  return absolute;
}

export function attemptArgv(plan, attempt, output) {
  const levels = `${Math.min(...attempt.levels)}-${Math.max(...attempt.levels)}`;
  const args = [BENCH,
    '--backend', attempt.stack,
    '--track', plan.definition.track,
    '--levels', levels,
    '--run-index', '0',
    '--out', output,
    '--agent-adapter', attempt.agentAdapter,
    '--model', attempt.model,
    '--guidance', attempt.guidance,
    '--fix-rounds', String(plan.definition.budgets.fixRounds),
    '--parent-attempt-id', attempt.id,
    '--no-media'];
  for (const pack of plan.definition.selection.packs) args.push('--pack', pack);
  for (const check of plan.definition.selection.checks) args.push('--check', check);
  if (attempt.skills.length) args.push('--skills', attempt.skills.join(','));
  if (plan.definition.budgets.maxCostUsdPerAttempt !== null) {
    args.push('--max-budget-usd', String(plan.definition.budgets.maxCostUsdPerAttempt));
  }
  return args;
}

function readAttemptResult(plan, attempt, output, processResult) {
  const runPath = join(output, 'run.json');
  let run = null;
  let artifactError = null;
  if (existsSync(runPath)) {
    try {
      run = readArtifactPayload(runPath, { expectedKind: 'benchmark_run' });
      const agent = plan.agents.find(item => item.adapter === attempt.agentAdapter
        && item.model === attempt.model && item.guidance === attempt.guidance
        && canonicalDefinitionJson(item.skills) === canonicalDefinitionJson(attempt.skills));
      const expectedLevels = [...attempt.levels].sort((a, b) => a - b);
      const actualLevels = (run.levels ?? []).map(level => level.level).sort((a, b) => a - b);
      if (run.artifactEnvelope.attempt.parentId !== attempt.id
        || run.track !== plan.definition.track
        || run.backend !== attempt.stack
        || run.model !== attempt.model
        || run.guidance !== attempt.guidance
        || canonicalDefinitionJson(run.selectionRequest) !== canonicalDefinitionJson(plan.definition.selection)
        || canonicalDefinitionJson(run.skills) !== canonicalDefinitionJson(attempt.skills)
        || canonicalDefinitionJson(actualLevels) !== canonicalDefinitionJson(expectedLevels)
        || run.artifactEnvelope.identities.agentAdapter?.sha256 !== agent?.identity.sha256
        || run.artifactEnvelope.identities.engine?.sha256 !== plan.identities.engine.sha256
        || run.artifactEnvelope.identities.stackAdapter?.id !== attempt.stack
        || run.artifactEnvelope.identities.stackAdapter?.version
          !== plan.stacks.find(item => item.id === attempt.stack)?.version
        || run.runtime?.buildImage !== (plan.definition.runtime.buildImage
          ?? processResult.buildImage)
        || (plan.definition.budgets.maxCostUsdPerAttempt !== null
          && (!Number.isFinite(run.totals?.costUsd)
            || run.totals.costUsd > plan.definition.budgets.maxCostUsdPerAttempt))) {
        throw new Error('run.json does not match its planned campaign attempt');
      }
    }
    catch (error) { artifactError = error; }
  }
  if (artifactError) return { exitCode: processResult.code, timedOut: processResult.timedOut,
    run: { outcome: { kind: 'harness_failure',
      reason: `run.json is invalid: ${artifactError.message}` } } };
  return { exitCode: processResult.code, timedOut: processResult.timedOut, run };
}

export function campaignExecutionEnvironment(plan, env = process.env) {
  const executionEnv = { ...env };
  if (plan.definition.runtime.buildImage !== null) {
    if (executionEnv.STACK_BENCH_IMAGE
      && executionEnv.STACK_BENCH_IMAGE !== plan.definition.runtime.buildImage) {
      throw new Error('ambient STACK_BENCH_IMAGE conflicts with the campaign build image');
    }
    executionEnv.STACK_BENCH_IMAGE = plan.definition.runtime.buildImage;
  }
  return executionEnv;
}

export function prepareCampaign(campaignFile, directory) {
  const plan = compileCampaignFile(resolve(campaignFile));
  const lock = acquireCampaignLock(directory, plan);
  try { return initializeCampaignDirectory(plan, directory); }
  finally { releaseCampaignLock(lock); }
}

export function reconcileCampaign(campaignFile, directory,
  { rescue = rescueSupervisedLease } = {}) {
  const plan = compileCampaignFile(resolve(campaignFile));
  const lock = acquireCampaignLock(directory, plan);
  try {
    const initialized = initializeCampaignDirectory(plan, directory);
    const { state } = readCampaignState(initialized.paths.root);
    const attempt = state.attempts.find(item => item.status === 'running');
    if (!attempt) throw new Error('campaign has no running attempt to reconcile');
    const execution = attempt.executions.at(-1);
    const output = contained(initialized.paths.root, execution.output, 'attempt output');
    const supervisorState = contained(initialized.paths.root,
      join('.private', `${execution.id}.supervisor.json`), 'supervisor state');
    if (!existsSync(supervisorState)) {
      throw new Error('running attempt has no private supervisor evidence; refusing to infer cleanup');
    }
    rescue(supervisorState, output);
    const reconciled = markInterruptedExecution(state, execution.id, {
      reason: 'controller ended before recording completion; exact-owned cleanup was proven',
    });
    writeCampaignState(initialized.paths.state, plan, reconciled);
    return reconciled;
  } finally { releaseCampaignLock(lock); }
}

export async function executeCampaign(campaignFile, directory,
  { allowDraft = false, env = process.env, execute = runBounded } = {}) {
  const plan = compileCampaignFile(resolve(campaignFile));
  if (plan.state !== 'frozen' && !allowDraft) {
    throw new Error('campaign execution requires a frozen plan; draft plans are inspection-only');
  }
  const executionEnv = campaignExecutionEnvironment(plan, env);
  const lock = acquireCampaignLock(directory, plan);
  try {
    const initialized = initializeCampaignDirectory(plan, directory);
    let { state } = readCampaignState(initialized.paths.root);
    if (state.attempts.some(attempt => attempt.status === 'running')) {
      throw new Error('campaign has an unresolved running attempt; prove its owned resources are clean before reconciliation');
    }
    const invalidAtStart = state.summary.invalid;
    while (true) {
      const next = claimNextAttempt(state);
      if (!next.claim) return next.state;
      state = next.state;
      writeCampaignState(initialized.paths.state, plan, state);
      const output = contained(initialized.paths.root, next.claim.output, 'attempt output');
      const supervisorState = contained(initialized.paths.root,
        join('.private', `${next.claim.executionId}.supervisor.json`), 'supervisor state');
      const processResult = await execute(process.execPath,
        attemptArgv(plan, next.claim.attempt, output), {
          cwd: ROOT,
          env: { ...executionEnv, STACK_BENCH_SUPERVISOR_STATE: supervisorState },
          stdio: 'inherit',
          timeoutMs: plan.definition.budgets.attemptTimeoutMinutes * 60_000,
        });
      processResult.buildImage = executionEnv.STACK_BENCH_IMAGE;
      let cleanupError = null;
      if (!processResult.ok && existsSync(supervisorState)) {
        try { rescueSupervisedLease(supervisorState, output); }
        catch (error) { cleanupError = error; }
      }
      const result = cleanupError
        ? { exitCode: processResult.code, timedOut: false, run: { outcome: {
          kind: 'harness_failure', reason: `attempt cleanup failed: ${cleanupError.message}` } } }
        : readAttemptResult(plan, next.claim.attempt, output, processResult);
      state = finishCampaignExecution(state, next.claim.executionId,
        result, {
          retries: plan.definition.attemptPolicy.retries,
          retryOn: plan.definition.attemptPolicy.retryOn,
        });
      writeCampaignState(initialized.paths.state, plan, state);
      if (state.summary.invalid > invalidAtStart) return state;
    }
  } finally {
    releaseCampaignLock(lock);
  }
}
