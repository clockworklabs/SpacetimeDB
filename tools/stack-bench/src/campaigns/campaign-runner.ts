import { setTimeout as delay } from 'node:timers/promises';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';


import { ARTIFACT_FILE, emptyArtifactIdentities, readArtifact, readArtifactPayload,
  writeArtifact } from '../evidence/artifacts.js';
import { durableCostLedger } from '../evidence/cost-proof.js';
import { acquireCampaignLock, releaseCampaignLock, watchCampaignCancellation } from './campaign-lock.js';
import { compileCampaignFile } from './campaign-compiler.js';
import type { CampaignAttemptPlan, CompiledCampaignPlan } from './campaign-compiler.js';
import { claimNextAttempt, finishCampaignExecution, initializeCampaignDirectory,
  markInterruptedExecution, readCampaignState, writeCampaignState } from './campaign-scheduler.js';
import type { CampaignClaim, CampaignDirectory, CampaignExecutionResult, CampaignState }
  from './campaign-scheduler.js';
import type { CampaignExtensionSeed } from './campaign-scheduler.js';
import { rescueSupervisedLease } from '../runtime/recovery.js';
import { runBounded } from '../runtime/bounded-process.js';
import { campaignTimeBudget, readTimeGrantRequests } from './campaign-scheduler.js';
import { timeContinuationEligibility } from '../progression/live-progression.js';
import { depthPauseDurationMs, readDepthPause } from './campaign-depth-pause.js';
import type { BoundedProcessResult, RunBoundedOptions }
  from '../runtime/bounded-process.js';
import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { RUN_INDEX_CAP } from '../composition/tracks.js';
import { readCampaignAdmission, runCampaignAdmission, delegateCampaignReservation,
  closeCampaignDelegation, releaseCampaignReservation, CampaignResourceUnavailable } from './campaign-admission.js';
import { recoverCampaignReservations } from './campaign-admission.js';
import { resolveExecutionCredentials, validateExecutionCredentialTargets } from '../agents/credential-profiles.js';
import type { ExecutionCredentials } from '../agents/credential-profiles.js';
import type { CampaignReservation } from './campaign-admission.js';
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
  ownershipMarkerSha256?: string;
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
  pausedMs?: number;
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
  runIndices: number[];
  reservation?: CampaignReservation;
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
  maxBudgetUsd: number | null | undefined = undefined,
  extension: CampaignExtensionSeed | null = null): string[] {
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
    if (extension !== null) {
      args.push('--seed-from', resolve(dirname(campaignPlanPath), extension.source),
        '--seed-through', String(extension.fromDepth),
        '--progression-seed-json', JSON.stringify(extension));
    }
  } else {
    if (progressionResumeFrom !== null || extension !== null) {
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
  executionId: string, output: string, processResult: RunnerProcessResult,
  extension: CampaignExtensionSeed | null = null): AttemptResult {
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
        progressionSeed: extension,
      });
    }
    catch (error) { artifactError = error instanceof Error ? error : new Error(String(error)); }
  }
  if (artifactError) {
    // Preserve the reported trigger for diagnosis, never as accepted evidence.
    const reported = run?.contaminated ? run.contamination?.verdict : run?.outcome?.reason;
    const processDetail = typeof reported === 'string' && reported.trim()
      ? reported.slice(0, 800) : processFailureDetail(processResult);
    return withRetryAuthority({ exitCode: processResult.code, timedOut: processResult.timedOut,
      run: { outcome: { kind: 'harness_failure',
        reason: `${processDetail ? `Reported failure: ${processDetail}; ` : ''}partial ${ARTIFACT_FILE.run} is invalid: ${artifactError.message}` } } });
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
    const ledger = durableCostLedger(run as Parameters<typeof durableCostLedger>[0], 'execution');
    if (!ledger.complete) {
      throw new Error(`cannot retry ${claim.attempt.id}: prior provider spend is unknown`);
    }
    spent += ledger.reportedCostUsd;
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
  const explicit = lines.filter(line => /^(?:Error:|ABORTED:|CONTAMINATED\b)/.test(line)).at(-1);
  return (explicit ?? lines.slice(-4).join(' | ')).slice(0, 800);
}

function assertAdmissionReferences(plan: CompiledCampaignPlan, directory: string,
  state: CampaignState, allowRelocatedEvidence = false): CampaignState {
  const ids = [...new Set(state.attempts.flatMap(attempt =>
    attempt.executions.map(execution => execution.admissionId)))];
  let recordedResultsDir: unknown;
  for (const id of ids) {
    const admission = readCampaignAdmission(directory, id, plan, { allowRelocatedEvidence });
    for (const attempt of state.attempts) {
      for (const execution of attempt.executions.filter(execution => execution.admissionId === id)) {
        if ((admission.attemptId !== undefined && admission.attemptId !== attempt.plan.id)
          || !admission.reports.some(report => report.request.runIndex === execution.runIndex)) {
          throw new Error(`campaign execution ${execution.id} does not match its admission`);
        }
      }
    }
    const origin = admission.reports[0]?.request.resultsDir;
    if (allowRelocatedEvidence && recordedResultsDir !== undefined && origin !== recordedResultsDir) {
      throw new Error('campaign admissions have different recorded results directories');
    }
    recordedResultsDir = origin;
    if (!admission.ok) throw new Error(`campaign execution references failed admission ${id}`);
  }
  return state;
}

export function inspectCampaign(directory: string, {
  requireCurrentInputs = true, allowRelocatedEvidence = false,
}: { requireCurrentInputs?: boolean; allowRelocatedEvidence?: boolean } = {}): CampaignInspection {
  const current = readCampaignState(directory, { requireCurrentInputs });
  return { ...current,
    state: assertAdmissionReferences(current.plan, current.paths.root, current.state, allowRelocatedEvidence) };
}

export function publicRecoveryProvesCleanup(output: string, backend: string, attemptId: string,
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
      || run.backendLease?.backend !== backend) return false;
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
    // External authenticated cleanup cannot rewrite the interrupted run. Its
    // ownership marker binds the release to that run's original lease instead.
    if (run.backendLease.state !== 'released'
      && (!run.backendLease.ownership?.markerSha256
        || recovery.ownershipMarkerSha256 !== run.backendLease.ownership.markerSha256)) return false;
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
    const recovered = recoverCampaignReservations(initialized.paths.root, plan);
    if (!running.length && !recovered) throw new Error('campaign has no running attempt or reservation to reconcile');
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
    admit = runCampaignAdmission, rescue = rescueSupervisedLease, signal = null,
    capacityPolicy = 'fail', onCapacityWait, executionCredentials }: {
      mode?: 'frozen' | 'model-free-trial';
      env?: NodeJS.ProcessEnv;
      execute?: ExecuteProcess;
      admit?: (plan: CompiledCampaignPlan, directory: string,
        options: { env: NodeJS.ProcessEnv; attempt: CampaignAttemptPlan; excludedRunIndices: number[]; signal?: AbortSignal }) => CampaignAdmissionAuthority | Promise<CampaignAdmissionAuthority>;
      rescue?: (supervisorState: string, output: string) => void;
      signal?: AbortSignal | null;
      capacityPolicy?: 'wait' | 'fail';
      onCapacityWait?: (reason: string | null) => void;
      executionCredentials?: ExecutionCredentials;
    } = {}): Promise<CampaignState> {
  const plan = compileCampaignFile(resolve(campaignFile));
  if (!['frozen', 'model-free-trial'].includes(mode)) {
    throw new Error(`unknown campaign execution mode ${JSON.stringify(mode)}`);
  }
  if (mode === 'frozen' && plan.state !== 'frozen') {
    throw new Error('campaign run requires a complete test plan; this plan is for inspection only');
  }
  if (mode === 'model-free-trial') {
    if (plan.state !== 'draft') {
      throw new Error('campaign trial requires a model-free draft; use campaign run for a complete test plan');
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
  if (capacityPolicy !== 'wait' && capacityPolicy !== 'fail') throw new Error('capacityPolicy must be wait or fail');
  validateExecutionCredentialTargets(executionCredentials, plan.agents.map(agent => agent.adapter),
    plan.attempts.map(attempt => attempt.id));
  const executionEnv = campaignExecutionEnvironment(plan, env);
  const lock = acquireCampaignLock(directory, plan);
  const cancellation = watchCampaignCancellation(lock, signal);
  signal = cancellation.signal;
  const active = new Map<string, Promise<{ claim: CampaignClaim; result: AttemptResult }>>();
  try {
    const initialized = initializeCampaignDirectory(plan, directory);
    let { state } = inspectCampaign(initialized.paths.root);
    if (state.attempts.some(attempt => attempt.status === 'running')) {
      throw new Error('campaign has an unresolved running attempt; prove its owned resources are clean before reconciliation');
    }
    if (!state.attempts.some(attempt => attempt.status === 'pending')) return state;
    const runClaim = async (claim: CampaignClaim, admission: CampaignAdmissionAuthority,
      attemptEnv: NodeJS.ProcessEnv): Promise<AttemptResult> => {
      const reservation = admission.reservation;
      const output = contained(initialized.paths.root, claim.output, 'attempt output');
      // Create every execution output before preflight bind-mounts it.
      mkdirSync(output, { recursive: true });
      contained(initialized.paths.root, claim.output, 'attempt output');
      const supervisorState = contained(initialized.paths.root,
        join('.private', `${claim.executionId}.supervisor.json`), 'supervisor state');
      let processResult: RunnerProcessResult;
      const pauseContext = plan.definition.mode.pauseAfterDepth === undefined ? null : {
        directory: initialized.paths.root, campaignSha256: plan.contentSha256,
        ownershipMarkerSha256: lock.record.ownershipMarkerSha256,
        attemptId: claim.attempt.id, executionId: claim.executionId,
        depth: plan.definition.mode.pauseAfterDepth,
      };
      let pausedMs = 0;
      try {
        const previous = state.attempts.find(a => a.plan.id === claim.attempt.id)!.executions.at(-2);
        if (previous?.timeContinuation) {
          const proof = timeContinuationEligibility(contained(initialized.paths.root, previous.output, 'time continuation'));
          if (!proof.eligible) throw new Error(proof.reason);
          if (proof.stateSha256 !== previous.timeContinuation.stateSha256) throw new Error('continuation checkpoint changed');
        }
        const delegationEnv = reservation ? delegateCampaignReservation(reservation,
          initialized.paths.root, { campaignSha256: plan.contentSha256, admissionId: admission.id,
            executionId: claim.executionId, output, backend: claim.attempt.stack, runIndex: claim.runIndex }) : {};
        const remainingBudget = remainingAttemptCostBudget(plan, claim, initialized.paths.root);
        const attemptState = () => state.attempts.find(a => a.plan.id === claim.attempt.id)!;
        const previousMs = campaignTimeBudget(plan, { ...attemptState(),
          executions: attemptState().executions.filter(e => e.id !== claim.executionId) }).consumedMs;
        const timeoutMs = campaignTimeBudget(plan, attemptState()).effectiveMinutes * 60_000 - previousMs;
        if (timeoutMs <= 0) throw new Error('attempt has no remaining duration allowance');
        const refreshTimeoutMs = (current: number, canExtend: boolean): number => {
          const attempt = attemptState();
          if (pauseContext) {
            pausedMs = depthPauseDurationMs(output, pauseContext);
            attempt.executions.at(-1)!.pausedMs = pausedMs;
          }
          for (const request of readTimeGrantRequests(initialized.paths.root)
            .filter(r => r.attemptId === claim.attempt.id)) {
            if (attempt.timeGrants?.some(g => g.request.grantId === request.grantId)) continue;
            const previousMinutes = campaignTimeBudget(plan, attempt).effectiveMinutes;
            const effectiveMinutes = previousMinutes + request.minutes;
            const reason = request.campaignSha256 !== plan.contentSha256 ? 'campaign identity changed'
              : request.executionId !== claim.executionId ? 'execution is no longer current'
                : !canExtend || signal?.aborted ? 'execution deadline or cancellation already reached'
                  : !Number.isSafeInteger(effectiveMinutes * 60_000 + Date.now()) ? 'time allowance overflow' : null;
            attempt.timeGrants ??= [];
            attempt.timeGrants.push(reason ? { request, disposition: 'rejected', reason }
              : { request, disposition: 'accepted', acceptedAt: new Date().toISOString(),
                previousMinutes, effectiveMinutes });
            // Persist acceptance before the process observes the new deadline.
            state.updatedAt = new Date().toISOString();
            writeCampaignState(initialized.paths.state, plan, state);
          }
          return Math.max(current, campaignTimeBudget(plan, attempt).effectiveMinutes * 60_000 - previousMs);
        };
        processResult = await execute(process.execPath,
        attemptArgv(plan, claim.attempt, output, claim.runIndex, initialized.paths.plan,
          claim.attempt.mode?.id !== 'dependency' || claim.resumeFrom === null ? null
            : contained(initialized.paths.root, claim.resumeFrom,
              'progression resume directory'), admission.id, remainingBudget,
          claim.extension), {
          cwd: ROOT,
          env: { ...campaignSlotEnvironment(attemptEnv, claim.attempt.stack, claim.runIndex),
            ...delegationEnv,
            STACK_BENCH_DEPTH_PAUSE_CONTEXT: pauseContext ? JSON.stringify(pauseContext) : '',
            STACK_BENCH_PROVIDER_WAIT_CONTEXT: JSON.stringify({ directory: initialized.paths.root,
              campaignSha256: plan.contentSha256, attemptId: claim.attempt.id,
              executionId: claim.executionId, ownershipMarkerSha256: lock.record.ownershipMarkerSha256,
              root: join(output, 'provider-waits') }),
            STACK_BENCH_SUPERVISOR_STATE: supervisorState },
          stdio: 'inherit',
          logs: { stdout: join(output, 'process.stdout.log'), stderr: join(output, 'process.stderr.log') },
          timeoutMs, refreshTimeoutMs,
          ...(pauseContext ? { pauseInterval: () => readDepthPause(output, pauseContext) } : {}),
          signal,
        });
        refreshTimeoutMs(timeoutMs, false);
        if (pauseContext && processResult.pausedMs !== undefined) {
          pausedMs = processResult.pausedMs;
          attemptState().executions.at(-1)!.pausedMs = pausedMs;
        }
        processResult.buildImage = attemptEnv.STACK_BENCH_IMAGE;
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
      return { ...readAttemptResult(plan, claim.attempt, claim.executionId, output, processResult,
        claim.extension), ...(pauseContext ? { pausedMs } : {}) };
    };
    cancellation.poll();
    const invalidAtStart = state.summary.invalid;
    let stopLaunching = signal?.aborted === true;
    let dispatchFailure: unknown;
    while (true) {
      let waitingForCapacity = false;
      try {
        while (!stopLaunching && !signal?.aborted && active.size < plan.summary.parallelism) {
          const pending = state.attempts.find(attempt => attempt.status === 'pending');
          if (!pending) break;
          const credentials = resolveExecutionCredentials(pending.plan.agentAdapter, pending.plan.id,
            executionCredentials ?? {}, executionEnv);
          const previousAssignment = pending.executions.at(-1)?.credentialAssignment;
          if (pending.executions.length && canonicalDefinitionJson(previousAssignment ?? null)
            !== canonicalDefinitionJson(credentials.assignment)) {
            throw new Error(`attempt ${pending.plan.id} credential assignment changed; start a fresh attempt`);
          }
          const credentialPath = contained(initialized.paths.root,
            join('.private', `${pending.plan.id}.credentials.json`), 'attempt credentials');
          const credentialPin = JSON.stringify({ assignment: credentials.assignment,
            fingerprint: credentials.env.STACK_BENCH_CREDENTIAL_SECRET_SHA256 ?? null });
          if (existsSync(credentialPath) && readFileSync(credentialPath, 'utf8') !== credentialPin) {
            throw new Error(`attempt ${pending.plan.id} credential profile changed; start a fresh attempt`);
          }
          let admission: CampaignAdmissionAuthority;
          try {
            admission = await admit(plan, initialized.paths.root, { env: credentials.env, attempt: pending.plan,
              signal: signal ?? undefined,
              excludedRunIndices: state.attempts.flatMap(attempt => attempt.executions
                .filter(execution => execution.status === 'running').map(execution => execution.runIndex)) });
          } catch (error) {
            if (signal?.aborted) break;
            if (!(error instanceof CampaignResourceUnavailable) || capacityPolicy === 'fail') throw error;
            onCapacityWait?.(error.message);
            if (active.size) { waitingForCapacity = true; break; }
            try { await delay(1000, undefined, { signal: signal ?? undefined }); }
            catch (error) { if (!signal?.aborted) throw error; }
            continue;
          }
          if (!admission?.payload?.ok || !admission.id) {
            throw new Error('attempt preflight admission failed; no attempt was claimed');
          }
          const reservation = admission.reservation;
          if (signal?.aborted) {
            if (reservation) releaseCampaignReservation(reservation);
            break;
          }
          let claim: CampaignClaim;
          try {
            onCapacityWait?.(null);
            const next = claimNextAttempt(state, { admissionId: admission.id, runIndex: admission.runIndices[0] });
            if (!next.claim) throw new Error('attempt dispatch has no available worker');
            state = next.state;
            claim = next.claim;
            state.attempts.find(attempt => attempt.plan.id === claim.attempt.id)!.executions.at(-1)!
              .credentialAssignment = credentials.assignment;
            mkdirSync(dirname(credentialPath), { recursive: true });
            if (!existsSync(credentialPath)) writeFileSync(credentialPath, credentialPin, { flag: 'wx', mode: 0o600 });
            writeCampaignState(initialized.paths.state, plan, state);
          } catch (error) {
            if (reservation) releaseCampaignReservation(reservation);
            throw error;
          }
          const promise = runClaim(claim, admission, credentials.env).then(result => {
            if (result.cleanupRequired) return { claim, result };
            if (reservation) {
              try {
                closeCampaignDelegation(initialized.paths.root, claim.executionId);
                releaseCampaignReservation(reservation);
              } catch (error) {
                return { claim, result: { cleanupRequired: true,
                  reason: `attempt reservation cleanup failed: ${error instanceof Error ? error.message : String(error)}` } };
              }
            }
            return { claim, result };
          }, error => ({ claim, result: { cleanupRequired: true,
            reason: `campaign worker failed; reservation retained: ${error instanceof Error
              ? error.message : String(error)}` } }));
          active.set(claim.executionId, promise);
        }
      } catch (error) {
        stopLaunching = true;
        dispatchFailure = error;
      }
      if (!active.size) {
        if (dispatchFailure) throw dispatchFailure;
        return state;
      }
      const completed = await Promise.race([
        ...active.values(),
        ...(waitingForCapacity ? [delay(1000, null, { signal: signal ?? undefined, ref: false })
          .catch(error => { if (!signal?.aborted) throw error; return null; })] : []),
      ]);
      if (!completed) continue;
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
    // An unexpected state-write failure must not free capacity under live children.
    await Promise.allSettled(active.values());
    cancellation.close();
    releaseCampaignLock(lock);
  }
}
