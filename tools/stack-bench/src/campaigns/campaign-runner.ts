import { existsSync, mkdirSync } from 'node:fs';
import { join, resolve } from 'node:path';


import { ARTIFACT_FILE, emptyArtifactIdentities, readArtifact, readArtifactPayload,
  writeArtifact } from '../evidence/artifacts.js';
import { acquireCampaignLock, releaseCampaignLock } from './campaign-lock.js';
import { compileCampaignFile } from './campaign-compiler.js';
import type { CampaignAttemptPlan, CompiledCampaignPlan } from './campaign-compiler.js';
import { claimNextAttempt, finishCampaignExecution, initializeCampaignDirectory,
  markInterruptedExecution, readCampaignState, writeCampaignState } from './campaign-scheduler.js';
import type { CampaignClaim, CampaignDirectory, CampaignExecutionResult, CampaignState }
  from './campaign-scheduler.js';
import { rescueSupervisedLease } from '../runtime/recovery.js';
import { runBounded } from '../runtime/bounded-process.js';
import type { BoundedProcessResult, RunBoundedOptions }
  from '../runtime/bounded-process.js';
import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { RUN_INDEX_CAP } from '../composition/tracks.js';
import { readCampaignAdmission, runCampaignAdmission } from './campaign-admission.js';
import { campaignChildPath as contained } from './campaign-path.js';
import { validateCampaignRun } from './campaign-run-validation.js';
import type { BenchmarkRun } from './campaign-run-validation.js';
import { campaignExecutionEnvironment, campaignSlotEnvironment } from './campaign-runtime.js';

import { STACK_BENCH_ROOT as ROOT, compiledEntrypoint } from '../package-root.js';
const BENCH = compiledEntrypoint('commands', 'bench.js');

type UnknownRecord = Record<string, unknown>;
const integer = (value: unknown): value is number =>
  typeof value === 'number' && Number.isInteger(value);
const finite = (value: unknown): value is number =>
  typeof value === 'number' && Number.isFinite(value);

interface RecoveryArtifact extends UnknownRecord {
  runId?: string;
  status?: string;
  backend?: string;
  cleanup?: { succeeded?: boolean; retained?: boolean };
  resources?: {
    backendState?: string;
    buildContainer?: { running?: boolean };
    listenerProcesses?: unknown[];
    locks?: Array<{ released?: boolean }>;
  };
}

interface CampaignProcessArtifact extends UnknownRecord {
  executionId?: string;
}

interface RunnerProcessResult {
  ok?: boolean;
  code: number | null;
  signal?: NodeJS.Signals | null;
  timedOut: boolean;
  cancelled?: boolean;
  error?: Error | null;
  logs?: BoundedProcessResult['logs'];
  stdoutTail?: string;
  stderrTail?: string;
  buildImage?: string | null;
}
type ExecuteProcess = (command: string, argv: string[], options: RunBoundedOptions & {
  signal?: AbortSignal | null;
}) => Promise<RunnerProcessResult>;

interface CampaignAdmissionAuthority {
  id: string;
  payload: { ok: boolean };
}

interface CampaignInspection extends CampaignDirectory {
  state: CampaignState;
}

interface RetryAuthority {
  transient: boolean;
  recoveryClean: boolean;
  budgetKnown: boolean;
  cause: string | null | undefined;
}

interface AttemptResult extends CampaignExecutionResult {
  run?: BenchmarkRun | null;
  retryAuthority?: RetryAuthority;
  cleanupRequired?: boolean;
  reason?: string;
}


export function attemptArgv(plan: CompiledCampaignPlan, attempt: CampaignAttemptPlan,
  output: string, runIndex: unknown = undefined, campaignPlanPath: string | null = null,
  progressionResumeFrom: string | null = null, campaignAdmissionId: string | null = null,
  maxBudgetUsd: number | null | undefined = undefined): string[] {
  if (!integer(runIndex) || runIndex < 0 || runIndex > RUN_INDEX_CAP) {
    throw new Error(`attempt ${attempt.id} requires a run slot from 0 through ${RUN_INDEX_CAP}`);
  }
  const dependencyMode = attempt.mode?.id === 'dependency';
  if (dependencyMode !== Boolean(attempt.dependencyPolicy)) {
    throw new Error(`attempt ${attempt.id} mode and dependency policy do not match`);
  }
  const hasFeatureCatalog = Boolean(plan.featureCatalog);
  if (hasFeatureCatalog !== Boolean(attempt.featureCatalog)) {
    throw new Error(`attempt ${attempt.id} feature catalog does not match its campaign`);
  }
  if (!attempt.condition?.guidance?.documents?.[attempt.stack]) {
    throw new Error(`attempt ${attempt.id} has no guidance document for ${attempt.stack}`);
  }
  const plannedPricing = { unit: plan.definition.pricing.unit,
    rates: plan.definition.pricing.models[attempt.model] };
  if (canonicalDefinitionJson(attempt.pricing) !== canonicalDefinitionJson(plannedPricing)) {
    throw new Error(`attempt ${attempt.id} pricing does not match its campaign`);
  }
  const args = [BENCH];
  if (typeof campaignPlanPath !== 'string' || !campaignPlanPath) {
    throw new Error(`attempt ${attempt.id} requires its compiled campaign plan path`);
  }
  args.push('--campaign-file', resolve(campaignPlanPath), '--campaign-attempt-id', attempt.id);
  if (campaignAdmissionId !== null) {
    if (typeof campaignAdmissionId !== 'string' || !campaignAdmissionId) {
      throw new Error(`attempt ${attempt.id} has an invalid campaign admission id`);
    }
    args.push('--campaign-admission-id', campaignAdmissionId);
  }
  if (hasFeatureCatalog) {
    if (canonicalDefinitionJson(attempt.featureCatalog)
      !== canonicalDefinitionJson(plan.featureCatalog?.identity)) {
      throw new Error(`attempt ${attempt.id} feature catalog identity does not match its campaign`);
    }
  }
  if (dependencyMode) {
    if (progressionResumeFrom !== null) {
      if (typeof progressionResumeFrom !== 'string' || !progressionResumeFrom) {
        throw new Error(`attempt ${attempt.id} has an invalid progression resume directory`);
      }
      args.push('--progression-resume-from', resolve(progressionResumeFrom));
    }
  } else {
    if (progressionResumeFrom !== null) {
      throw new Error(`strict attempt ${attempt.id} cannot resume dependency progression state`);
    }
  }
  args.push('--run-index', String(runIndex),
    '--out', output);
  const plannedBudget = plan.definition.budgets.maxCostUsdPerAttempt;
  const executionBudget = maxBudgetUsd === undefined ? plannedBudget : maxBudgetUsd;
  if (executionBudget !== null) {
    if (!Number.isFinite(executionBudget) || executionBudget <= 0
      || (plannedBudget !== null && executionBudget > plannedBudget)) {
      throw new Error(`attempt ${attempt.id} has an invalid remaining cost budget`);
    }
    args.push('--max-budget-usd', String(Number(executionBudget.toFixed(6))));
  }
  return args;
}


const TRANSIENT_PROVIDER_STATUSES = new Set([500, 502, 503, 504, 529]);

export function campaignRetryAuthority(run: BenchmarkRun | null | undefined, {
  recoveryClean = false, requireCostReceipt = false,
}: { recoveryClean?: boolean; requireCostReceipt?: boolean } = {}): RetryAuthority {
  const outcome = run?.outcome;
  const providerStatus = outcome?.provider?.providerStatus;
  const providerTransient = outcome?.kind === 'provider_failure'
    && outcome.phase === 'coding-session'
    && outcome.reason !== 'provider-throttle-exhausted'
    && ((typeof providerStatus === 'number' && TRANSIENT_PROVIDER_STATUSES.has(providerStatus))
      || ['provider-api-error', 'provider-connection-error'].includes(outcome.reason ?? ''));
  const transient = providerTransient;
  const cost = run?.totals?.costUsd;
  const budgetKnown = !requireCostReceipt || (run?.totals?.costComplete === true
    && finite(cost) && cost >= 0);
  return {
    transient,
    recoveryClean: recoveryClean === true,
    budgetKnown,
    cause: transient
      ? providerStatus === null || providerStatus === undefined
        ? outcome.reason : `provider-http-${providerStatus}`
      : null,
  };
}

function readAttemptResult(plan: CompiledCampaignPlan, attempt: CampaignAttemptPlan,
  executionId: string, output: string, processResult: RunnerProcessResult): AttemptResult {
  const withRetryAuthority = (result: AttemptResult): AttemptResult => ({ ...result,
    retryAuthority: campaignRetryAuthority(result.run, {
      recoveryClean: publicRecoveryProvesCleanup(output, attempt.stack, attempt.id,
        executionId, result.run ?? null),
      requireCostReceipt: plan.definition.budgets.maxCostUsdPerAttempt !== null,
    }) });
  if (processResult.cancelled === true) {
    return withRetryAuthority({ exitCode: processResult.code, timedOut: false,
      run: { outcome: { kind: 'scheduler_interrupted',
        reason: 'campaign cancellation requested' } } });
  }
  const runPath = join(output, ARTIFACT_FILE.run);
  let run = null;
  let artifactError = null;
  if (existsSync(runPath)) {
    try {
      run = readArtifactPayload(runPath, { expectedKind: 'benchmark_run' }) as BenchmarkRun;
      validateCampaignRun(plan, attempt, run, {
        buildImage: processResult.buildImage,
        resultDir: output,
      });
    }
    catch (error) { artifactError = error instanceof Error ? error : new Error(String(error)); }
  }
  if (artifactError) {
    const processDetail = processResult.code !== 0
      ? processFailureDetail(processResult) : null;
    return withRetryAuthority({ exitCode: processResult.code, timedOut: processResult.timedOut,
      run: { outcome: { kind: 'harness_failure',
        reason: `${processDetail ? `${processDetail}; ` : ''}partial ${ARTIFACT_FILE.run} is invalid: ${artifactError.message}` } } });
  }
  if (!run && processResult.code !== 0 && !processResult.timedOut) {
    const detail = processFailureDetail(processResult);
    return withRetryAuthority({ exitCode: processResult.code, timedOut: false, run: { outcome: {
      kind: 'harness_failure', reason: detail || `attempt ended before producing ${ARTIFACT_FILE.run}` } } });
  }
  return withRetryAuthority({ exitCode: processResult.code, timedOut: processResult.timedOut, run });
}

export function remainingAttemptCostBudget(
  plan: { definition: { budgets: { maxCostUsdPerAttempt: number | null } } },
  claim: { attempt: { id: string }; priorOutputs?: string[] },
  directory: string): number | null {
  const cap = plan.definition.budgets.maxCostUsdPerAttempt;
  if (cap === null) return null;
  let spent = 0;
  for (const output of claim.priorOutputs ?? []) {
    const runPath = join(contained(directory, output, 'prior attempt output'), ARTIFACT_FILE.run);
    if (!existsSync(runPath)) {
      throw new Error(`cannot retry ${claim.attempt.id}: prior provider spend is unknown`);
    }
    const run = readArtifactPayload(runPath, { expectedKind: 'benchmark_run' }) as BenchmarkRun;
    const cost = run.totals?.costUsd;
    if (run.totals?.costComplete !== true || !finite(cost) || cost < 0) {
      throw new Error(`cannot retry ${claim.attempt.id}: prior provider spend is unknown`);
    }
    spent += cost;
  }
  const remaining = Number((cap - spent).toFixed(6));
  if (remaining <= 0) {
    throw new Error(`cannot retry ${claim.attempt.id}: its $${cap} cost cap is exhausted`);
  }
  return remaining;
}

export function processFailureDetail(processResult: Partial<RunnerProcessResult>): string {
  const text = processResult.stderrTail || processResult.stdoutTail
    || processResult.error?.message || '';
  const lines = text.split(/\r?\n/).map(line => line.trim()).filter(Boolean);
  const explicit = lines.filter(line => /^Error:\s+/.test(line)).at(-1);
  return (explicit ?? lines.slice(-4).join(' | ')).slice(0, 800);
}

function assertAdmissionReferences(plan: CompiledCampaignPlan, directory: string,
  state: CampaignState): CampaignState {
  const ids = [...new Set(state.attempts.flatMap(attempt =>
    attempt.executions.map(execution => execution.admissionId)))];
  for (const id of ids) {
    const admission = readCampaignAdmission(directory, id, plan);
    if (!admission.ok) throw new Error(`campaign execution references failed admission ${id}`);
  }
  return state;
}

export function inspectCampaign(directory: string, {
  requireCurrentInputs = true,
}: { requireCurrentInputs?: boolean } = {}): CampaignInspection {
  const current = readCampaignState(directory, { requireCurrentInputs });
  return { ...current,
    state: assertAdmissionReferences(current.plan, current.paths.root, current.state) };
}

function publicRecoveryProvesCleanup(output: string, backend: string, attemptId: string,
  executionId: string, currentRun: BenchmarkRun | null = null): boolean {
  const runPath = join(output, ARTIFACT_FILE.run);
  const processPath = join(output, ARTIFACT_FILE.process);
  const recoveryPath = join(output, ARTIFACT_FILE.recovery);
  if ((!currentRun && !existsSync(runPath)) || !existsSync(processPath) || !existsSync(recoveryPath)) {
    return false;
  }
  try {
    const run = currentRun ?? readArtifactPayload<BenchmarkRun>(runPath,
      { expectedKind: 'benchmark_run' });
    if (typeof run.id !== 'string' || !run.id
      || run.backend !== backend
      || run.artifactEnvelope?.attempt?.parentId !== attemptId
      || run.backendLease?.runId !== run.id
      || run.backendLease?.backend !== backend
      || run.backendLease?.state !== 'released') return false;
    const processArtifact = readArtifact<CampaignProcessArtifact>(processPath, {
      expectedKind: 'campaign_process', expectedId: `${executionId}-process`,
    });
    if (processArtifact.attempt.id !== executionId
      || processArtifact.attempt.parentId !== attemptId
      || processArtifact.payload.executionId !== executionId) return false;
    const artifact = readArtifact<RecoveryArtifact>(recoveryPath, {
      expectedKind: 'recovery', expectedId: `${run.id}-recovery`,
    });
    const recovery = artifact.payload;
    if (artifact.attempt.id !== `${run.id}-recovery`
      || artifact.attempt.parentId !== run.id) return false;
    return recovery.status === 'clean'
      && recovery.runId === run.id
      && recovery.backend === backend
      && recovery.cleanup?.succeeded === true
      && recovery.cleanup?.retained === false
      && recovery.resources?.backendState === 'released'
      && recovery.resources?.buildContainer?.running !== true
      && Array.isArray(recovery.resources?.listenerProcesses)
      && recovery.resources.listenerProcesses.length === 0
      && Array.isArray(recovery.resources?.locks)
      && recovery.resources.locks.every(resource => resource.released === true);
  } catch {
    return false;
  }
}

export function prepareCampaign(campaignFile: string, directory: string): CampaignInspection {
  const plan = compileCampaignFile(resolve(campaignFile));
  const lock = acquireCampaignLock(directory, plan);
  try { return initializeCampaignDirectory(plan, directory); }
  finally { releaseCampaignLock(lock); }
}

export function reconcileCampaign(campaignFile: string, directory: string,
  { rescue = rescueSupervisedLease }: {
    rescue?: (supervisorState: string, output: string) => void;
  } = {}): CampaignState {
  const plan = compileCampaignFile(resolve(campaignFile));
  const lock = acquireCampaignLock(directory, plan);
  try {
    const initialized = initializeCampaignDirectory(plan, directory);
    const { state } = inspectCampaign(initialized.paths.root);
    const running = state.attempts.filter(item => item.status === 'running');
    if (!running.length) throw new Error('campaign has no running attempt to reconcile');
    for (const attempt of running) {
      const execution = attempt.executions.at(-1)!;
      const output = contained(initialized.paths.root, execution.output, 'attempt output');
      const supervisorState = contained(initialized.paths.root,
        join('.private', `${execution.id}.supervisor.json`), 'supervisor state');
      if (existsSync(supervisorState)) {
        rescue(supervisorState, output);
      } else if (!publicRecoveryProvesCleanup(output, attempt.plan.stack, attempt.plan.id,
        execution.id)) {
        throw new Error('running attempt has neither private supervisor authority nor public clean recovery proof');
      }
    }
    let reconciled = state;
    for (const attempt of running) {
      reconciled = markInterruptedExecution(reconciled, attempt.executions.at(-1)!.id, {
        reason: 'controller ended before recording completion; exact-owned cleanup was proven',
      });
    }
    writeCampaignState(initialized.paths.state, plan, reconciled);
    return reconciled;
  } finally { releaseCampaignLock(lock); }
}

export async function executeCampaign(campaignFile: string, directory: string,
  { mode = 'frozen', env = process.env, execute = runBounded as ExecuteProcess,
    admit = runCampaignAdmission, rescue = rescueSupervisedLease, signal = null }: {
      mode?: 'frozen' | 'model-free-trial';
      env?: NodeJS.ProcessEnv;
      execute?: ExecuteProcess;
      admit?: (plan: CompiledCampaignPlan, directory: string,
        options: { env: NodeJS.ProcessEnv }) => CampaignAdmissionAuthority;
      rescue?: (supervisorState: string, output: string) => void;
      signal?: AbortSignal | null;
    } = {}): Promise<CampaignState> {
  const plan = compileCampaignFile(resolve(campaignFile));
  if (!['frozen', 'model-free-trial'].includes(mode)) {
    throw new Error(`unknown campaign execution mode ${JSON.stringify(mode)}`);
  }
  if (mode === 'frozen' && plan.state !== 'frozen') {
    throw new Error('campaign execution requires a frozen plan; draft plans are inspection-only');
  }
  if (mode === 'model-free-trial') {
    if (plan.state !== 'draft') {
      throw new Error('campaign trial requires a draft plan; use campaign run for a frozen plan');
    }
    const billable = plan.agents.filter(agent => agent.costLimit !== 'non-billable');
    if (billable.length) {
      throw new Error(`campaign trial requires non-billable agent adapters; found ${billable
        .map(agent => agent.adapter).join(', ')}`);
    }
    const nonzeroPricing = plan.agents.filter(agent => Object.values(
      plan.definition.pricing.models[agent.model] ?? {}).some(value => value !== 0));
    if (nonzeroPricing.length) {
      throw new Error(`campaign trial requires zero pricing for every selected model; found ${nonzeroPricing
        .map(agent => agent.model).join(', ')}`);
    }
  }
  const executionEnv = campaignExecutionEnvironment(plan, env);
  const lock = acquireCampaignLock(directory, plan);
  try {
    const initialized = initializeCampaignDirectory(plan, directory);
    let { state } = inspectCampaign(initialized.paths.root);
    if (state.attempts.some(attempt => attempt.status === 'running')) {
      throw new Error('campaign has an unresolved running attempt; prove its owned resources are clean before reconciliation');
    }
    if (!state.attempts.some(attempt => attempt.status === 'pending')) return state;
    const admission = admit(plan, initialized.paths.root, { env: executionEnv });
    if (!admission?.payload?.ok || typeof admission.id !== 'string' || !admission.id) {
      throw new Error('campaign-wide preflight admission failed; no attempt was claimed');
    }
    const runClaim = async (claim: CampaignClaim): Promise<AttemptResult> => {
      const output = contained(initialized.paths.root, claim.output, 'attempt output');
      // Create every execution output before preflight bind-mounts it.
      mkdirSync(output, { recursive: true });
      contained(initialized.paths.root, claim.output, 'attempt output');
      const supervisorState = contained(initialized.paths.root,
        join('.private', `${claim.executionId}.supervisor.json`), 'supervisor state');
      let processResult: RunnerProcessResult;
      try {
        const remainingBudget = remainingAttemptCostBudget(plan, claim, initialized.paths.root);
        processResult = await execute(process.execPath,
        attemptArgv(plan, claim.attempt, output, claim.runIndex, initialized.paths.plan,
          claim.attempt.mode?.id !== 'dependency' || claim.resumeFrom === null ? null
            : contained(initialized.paths.root, claim.resumeFrom,
              'progression resume directory'), admission.id, remainingBudget), {
          cwd: ROOT,
          env: { ...campaignSlotEnvironment(executionEnv, claim.attempt.stack, claim.runIndex),
            STACK_BENCH_SUPERVISOR_STATE: supervisorState },
          stdio: 'inherit',
          logs: { stdout: join(output, 'process.stdout.log'), stderr: join(output, 'process.stderr.log') },
          timeoutMs: plan.definition.budgets.attemptTimeoutMinutes * 60_000,
          signal,
        });
        processResult.buildImage = executionEnv.STACK_BENCH_IMAGE;
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        const reason = `attempt launcher failed: ${message}`;
        if (existsSync(supervisorState)) {
          try { rescue(supervisorState, output); }
          catch (cleanupError) {
            const cleanupMessage = cleanupError instanceof Error
              ? cleanupError.message : String(cleanupError);
            return { cleanupRequired: true,
              reason: `${reason}; cleanup failed: ${cleanupMessage}` };
          }
        }
        return { exitCode: null, timedOut: false,
          run: { outcome: { kind: 'harness_failure', reason } } };
      }
      try {
        writeArtifact(join(output, ARTIFACT_FILE.process), { kind: 'campaign_process',
          id: `${claim.executionId}-process`,
          attempt: { id: claim.executionId, parentId: claim.attempt.id },
          identities: emptyArtifactIdentities({ experiment: {
            id: plan.id, version: plan.version, sha256: plan.contentSha256, state: plan.state,
          } }),
          payload: { schemaVersion: 1, executionId: claim.executionId, runIndex: claim.runIndex,
            exitCode: processResult.code ?? null, signal: processResult.signal ?? null,
            timedOut: processResult.timedOut === true,
            streams: processResult.logs ? Object.fromEntries(Object.entries(processResult.logs)
              .map(([name, log]) => [name, { ...log, path: `process.${name}.log` }])) : null } });
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        const reason = `could not record campaign process evidence: ${message}`;
        if (existsSync(supervisorState)) {
          try { rescue(supervisorState, output); }
          catch (cleanupError) {
            const cleanupMessage = cleanupError instanceof Error
              ? cleanupError.message : String(cleanupError);
            return { cleanupRequired: true,
              reason: `${reason}; cleanup failed: ${cleanupMessage}` };
          }
        }
        return { exitCode: processResult.code ?? null, timedOut: false,
          run: { outcome: { kind: 'harness_failure', reason } } };
      }
      let cleanupError: Error | null = null;
      if (!processResult.ok && existsSync(supervisorState)) {
        try { rescue(supervisorState, output); }
        catch (error) {
          cleanupError = error instanceof Error ? error : new Error(String(error));
        }
      }
      if (cleanupError) {
        return { cleanupRequired: true,
          reason: `attempt cleanup failed: ${cleanupError.message}` };
      }
      return readAttemptResult(plan, claim.attempt, claim.executionId, output, processResult);
    };
    const active = new Map<string, Promise<{ claim: CampaignClaim; result: AttemptResult }>>();
    const invalidAtStart = state.summary.invalid;
    let stopLaunching = signal?.aborted === true;
    while (true) {
      while (!stopLaunching && !signal?.aborted && active.size < plan.summary.parallelism) {
        const next = claimNextAttempt(state, { admissionId: admission.id });
        state = next.state;
        if (!next.claim) break;
        const claim = next.claim;
        writeCampaignState(initialized.paths.state, plan, state);
        const promise = runClaim(claim).then(result => ({ claim, result }),
          error => ({ claim, result: { exitCode: null, timedOut: false,
            run: { outcome: { kind: 'harness_failure',
              reason: `campaign worker failed: ${error instanceof Error
                ? error.message : String(error)}` } } } }));
        active.set(claim.executionId, promise);
      }
      if (!active.size) return state;
      const completed = await Promise.race(active.values());
      active.delete(completed.claim.executionId);
      if (signal?.aborted) stopLaunching = true;
      if (completed.result.cleanupRequired === true) {
        // Keep the execution running in durable state. Its private supervisor
        // authority still exists, so reconcile can retry exact-owned cleanup.
        // Marking it invalid here would strand that authority permanently.
        stopLaunching = true;
        continue;
      }
      state = finishCampaignExecution(state, completed.claim.executionId,
        completed.result, {
          retries: plan.definition.attemptPolicy.retries,
          retryOn: plan.definition.attemptPolicy.retryOn,
        });
      writeCampaignState(initialized.paths.state, plan, state);
      if (state.summary.invalid > invalidAtStart) stopLaunching = true;
    }
  } finally {
    releaseCampaignLock(lock);
  }
}
