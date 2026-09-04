import { existsSync, mkdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { z } from 'zod';

import { ARTIFACT_FILE, emptyArtifactIdentities, readArtifact, writeArtifact }
  from '../evidence/artifacts.js';
import { campaignIdentity, validateCompiledCampaignPlan } from './campaign-compiler.js';
import type { CampaignAttemptPlan, CompiledCampaignPlan } from './campaign-compiler.js';
import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { RUN_INDEX_CAP } from '../composition/tracks.js';
import { formatZodError } from '../zod-error.js';
import { CAMPAIGN_FILE } from './campaign-path.js';

export const CAMPAIGN_STATE_SCHEMA_VERSION = 2;
type AttemptStatus = 'pending' | 'running' | 'completed' | 'invalid';
type ExecutionStatus = 'running' | 'completed' | 'invalid';
type CampaignStatus = 'prepared' | 'running' | 'completed' | 'attention-required';
type TerminalOutcome = 'passed' | 'app_failure';
type InvalidOutcome = 'provider_failure' | 'harness_failure' | 'inconclusive'
  | 'ungraded' | 'contaminated' | 'timed_out' | 'missing_artifact'
  | 'scheduler_interrupted';
type CampaignOutcome = TerminalOutcome | InvalidOutcome;

export interface CampaignRetry {
  requested: boolean;
  transient: boolean;
  recoveryClean: boolean;
  budgetKnown: boolean;
  scheduled: boolean;
  cause: string | null;
  reason: string;
}

export interface CampaignContinuation {
  grantId: string;
  level: number;
  nodeIds: string[];
  repairs: number;
  stateSha256: string;
  resumeFrom: string;
  scheduledAt: string;
}

export interface CampaignExtensionSeed {
  fromDepth: number;
  source: string;
  sourceSha256: string;
  sourceFiles: number;
  parent: {
    campaignId: string;
    campaignSha256: string;
    attemptId: string;
    executionId: string;
    runId: string;
    runSha256: string;
  };
}

export interface CampaignExecution {
  id: string;
  ordinal: number;
  status: string;
  output: string;
  startedAt: string;
  completedAt: string | null;
  exitCode: number | null;
  outcome: CampaignOutcome | null;
  reason: string | null;
  admissionId: string;
  runIndex: number;
  retry: CampaignRetry | null;
  continuation?: CampaignContinuation;
}

export interface CampaignAttemptState {
  plan: CampaignAttemptPlan;
  status: string;
  executions: CampaignExecution[];
  extension?: CampaignExtensionSeed;
}

export interface CampaignSummary {
  total: number;
  pending: number;
  running: number;
  completed: number;
  invalid: number;
  executions: number;
  [key: string]: number;
}

export interface CampaignState {
  schemaVersion: number;
  campaignId: string;
  campaignSha256: string;
  status: CampaignStatus;
  createdAt: string;
  updatedAt: string;
  maxParallel: number;
  attempts: CampaignAttemptState[];
  summary: CampaignSummary;
}

export interface CampaignClaim {
  attempt: CampaignAttemptPlan;
  executionId: string;
  output: string;
  runIndex: number;
  resumeFrom: string | null;
  priorOutputs: string[];
  extension: CampaignExtensionSeed | null;
}

interface RunArtifact {
  contaminated?: boolean;
  contamination?: { verdict?: string };
  outcome?: { kind?: string; reason?: string | null; inconclusive?: unknown };
}

export interface CampaignExecutionResult {
  exitCode?: number | null;
  timedOut?: boolean;
  run?: RunArtifact | null;
  retryAuthority?: {
    transient?: boolean;
    recoveryClean?: boolean;
    budgetKnown?: boolean;
    cause?: string | null;
  };
}

const ATTEMPT_STATUSES = new Set<AttemptStatus>(['pending', 'running', 'completed', 'invalid']);
const TERMINAL_OUTCOMES = new Set<string>(['passed', 'app_failure']);
const INVALID_OUTCOMES = new Set<string>(['provider_failure', 'harness_failure', 'inconclusive',
  'ungraded', 'incomplete', 'contaminated', 'timed_out', 'missing_artifact', 'scheduler_interrupted']);
const SAFE_ID = /^[a-z0-9][a-z0-9.-]*$/;
const FEATURE_ID = /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/;
const HASH = /^[a-f0-9]{64}$/;
const fail = (message: string): never => { throw new Error(`invalid campaign state: ${message}`); };

const timestampSchema = z.string().refine(value => !Number.isNaN(Date.parse(value)),
  'must be an ISO timestamp');
const retrySchema = z.strictObject({
  requested: z.boolean(),
  transient: z.boolean(),
  recoveryClean: z.boolean(),
  budgetKnown: z.boolean(),
  scheduled: z.boolean(),
  cause: z.string().min(1).nullable(),
  reason: z.string().min(1),
});
const continuationSchema = z.strictObject({
  grantId: z.string().min(1),
  level: z.number().int(),
  nodeIds: z.array(z.string()),
  repairs: z.number().int(),
  stateSha256: z.string(),
  resumeFrom: z.string(),
  scheduledAt: timestampSchema,
});
const extensionSeedSchema = z.strictObject({
  fromDepth: z.number().int().positive(),
  source: z.string().min(1),
  sourceSha256: z.string().regex(HASH),
  sourceFiles: z.number().int().nonnegative(),
  parent: z.strictObject({
    campaignId: z.string().min(1),
    campaignSha256: z.string().regex(HASH),
    attemptId: z.string().min(1),
    executionId: z.string().min(1),
    runId: z.string().min(1),
    runSha256: z.string().regex(HASH),
  }),
});

export function validateCampaignExtensionSeed(input: unknown): CampaignExtensionSeed {
  const parsed = extensionSeedSchema.safeParse(structuredClone(input));
  if (!parsed.success) {
    throw new Error(`invalid campaign extension: ${formatZodError(parsed.error, 'extension')}`);
  }
  return parsed.data;
}
const executionSchema = z.strictObject({
  id: z.string().min(1),
  ordinal: z.number().int(),
  status: z.enum(['running', 'completed', 'invalid']),
  output: z.string().min(1),
  startedAt: timestampSchema,
  completedAt: timestampSchema.nullable(),
  exitCode: z.number().int().nullable(),
  outcome: z.enum(['passed', 'app_failure', 'provider_failure', 'harness_failure',
    'inconclusive', 'ungraded', 'incomplete', 'contaminated', 'timed_out', 'missing_artifact',
    'scheduler_interrupted']).nullable(),
  reason: z.string().min(1).nullable(),
  admissionId: z.string().min(1),
  runIndex: z.number().int(),
  retry: retrySchema.nullable().optional(),
  continuation: continuationSchema.optional(),
});
const campaignStateSchema = z.strictObject({
  schemaVersion: z.literal(CAMPAIGN_STATE_SCHEMA_VERSION),
  campaignId: z.string().min(1),
  campaignSha256: z.string().regex(HASH),
  status: z.enum(['prepared', 'running', 'completed', 'attention-required']),
  createdAt: timestampSchema,
  updatedAt: timestampSchema,
  maxParallel: z.number().int(),
  attempts: z.array(z.strictObject({
    plan: z.looseObject({ id: z.string().min(1) }),
    status: z.enum(['pending', 'running', 'completed', 'invalid']),
    executions: z.array(executionSchema),
    extension: extensionSeedSchema.optional(),
  })).min(1),
  summary: z.strictObject({
    total: z.number().int(),
    pending: z.number().int(),
    running: z.number().int(),
    completed: z.number().int(),
    invalid: z.number().int(),
    executions: z.number().int(),
  }),
});

function string(value: unknown, at: string): string {
  if (typeof value !== 'string' || !value) fail(`${at} is required`);
  return value as string;
}

function recalculate(state: CampaignState, now: string): CampaignState {
  const counts = Object.fromEntries([...ATTEMPT_STATUSES].sort().map(status =>
    [status, state.attempts.filter(attempt => attempt.status === status).length])) as
    Record<AttemptStatus, number>;
  state.summary = { total: state.attempts.length, ...counts,
    executions: state.attempts.reduce((total, attempt) => total + attempt.executions.length, 0) };
  if (counts.running) state.status = 'running';
  else if (counts.invalid) state.status = 'attention-required';
  else if (counts.pending) state.status = 'prepared';
  else state.status = 'completed';
  state.updatedAt = now;
  return state;
}

export function validateCampaignState(input: unknown): CampaignState {
  const parsed = campaignStateSchema.safeParse(structuredClone(input));
  if (!parsed.success) {
    fail(formatZodError(parsed.error, 'document'));
  }
  const state = parsed.data as unknown as CampaignState;
  if (!Number.isInteger(state.maxParallel) || state.maxParallel < 1
    || state.maxParallel > RUN_INDEX_CAP + 1) {
    fail(`maxParallel must be an integer from 1 through ${RUN_INDEX_CAP + 1}`);
  }
  if (Date.parse(state.updatedAt) < Date.parse(state.createdAt)) fail('updatedAt precedes createdAt');
  const ids = new Set<string>();
  let running = 0;
  const runningSlots = new Set<number>();
  for (const [index, attempt] of state.attempts.entries()) {
    const at = `attempts[${index}]`;
    if (!SAFE_ID.test(attempt.plan.id)) fail(`${at}.plan.id is not a safe path component`);
    if (ids.has(attempt.plan.id)) fail(`${at}.plan.id duplicates ${attempt.plan.id}`);
    ids.add(attempt.plan.id);
    if (attempt.extension !== undefined) {
      const extensionAt = `${at}.extension`;
      if (attempt.plan.mode?.id !== 'dependency') {
        fail(`${extensionAt} requires dependency mode`);
      }
      if (!attempt.plan.levels.includes(attempt.extension.fromDepth)
        || !attempt.plan.levels.some(level => level > attempt.extension!.fromDepth)) {
        fail(`${extensionAt}.fromDepth must precede another planned depth`);
      }
      const expectedSource = `.private/extensions/${attempt.plan.id}/source`;
      if (attempt.extension.source !== expectedSource) {
        fail(`${extensionAt}.source is not the exact extension source directory`);
      }
    }
    let previousCompletedAt = null;
    for (const [executionIndex, execution] of attempt.executions.entries()) {
      const executionAt = `${at}.executions[${executionIndex}]`;
      if (!Number.isInteger(execution.runIndex) || execution.runIndex < 0
        || execution.runIndex > RUN_INDEX_CAP) fail(`${executionAt}.runIndex is invalid`);
      if (execution.ordinal !== executionIndex + 1) fail(`${executionAt}.ordinal is not contiguous`);
      if (execution.id !== `${attempt.plan.id}-execution${execution.ordinal}`) {
        fail(`${executionAt}.id does not match its attempt and ordinal`);
      }
      if (executionIndex < attempt.executions.length - 1 && execution.status !== 'invalid'
        && !(execution.status === 'completed' && execution.continuation !== undefined)) {
        fail(`${executionAt} is historical but not invalid`);
      }
      if (execution.output !== `attempts/${attempt.plan.id}/execution-${execution.ordinal}`) {
        fail(`${executionAt}.output is not the exact execution directory`);
      }
      if (Date.parse(execution.startedAt) < Date.parse(state.createdAt)) {
        fail(`${executionAt}.startedAt precedes campaign creation`);
      }
      if (previousCompletedAt && Date.parse(execution.startedAt) < Date.parse(previousCompletedAt)) {
        fail(`${executionAt}.startedAt overlaps the previous execution`);
      }
      if (execution.status === 'running') {
        running += 1;
        if (runningSlots.has(execution.runIndex)) fail(`${executionAt}.runIndex is already in use`);
        runningSlots.add(execution.runIndex);
        if (execution.completedAt !== null || execution.exitCode !== null
          || execution.outcome !== null || execution.reason !== null
          || (execution.retry !== undefined && execution.retry !== null)) {
          fail(`${executionAt} running fields are inconsistent`);
        }
      } else {
        const completedAt = execution.completedAt!;
        if (Date.parse(completedAt) < Date.parse(execution.startedAt)) {
          fail(`${executionAt}.completedAt precedes startedAt`);
        }
        previousCompletedAt = completedAt;
        const outcome = execution.outcome!;
        if (execution.status === 'completed'
          && (execution.exitCode !== 0 || execution.reason !== null)) fail(`${executionAt} completed fields are inconsistent`);
        if (execution.status === 'completed' && !TERMINAL_OUTCOMES.has(outcome)) {
          fail(`${executionAt}.outcome is not terminal`);
        }
        if (execution.status === 'invalid') {
          if (!INVALID_OUTCOMES.has(outcome)) fail(`${executionAt}.outcome is not invalid`);
          if (execution.retry !== undefined) {
            if (execution.retry === null) {
              throw new Error(`invalid campaign state: ${executionAt}.retry must be an object`);
            }
            const retry = execution.retry;
            if (retry.scheduled === true
              && (retry.requested !== true || retry.transient !== true
                || retry.recoveryClean !== true || retry.budgetKnown !== true)) {
              fail(`${executionAt}.retry is scheduled without transient clean-recovery authority`);
            }
          }
        }
      }
      if (execution.continuation !== undefined) {
        const continuation = execution.continuation;
        const continuationAt = `${executionAt}.continuation`;
        if (attempt.plan.mode?.id !== 'dependency' || execution.status !== 'completed'
          || continuation === undefined) {
          fail(`${continuationAt} requires a completed dependency execution`);
        }
        if (!SAFE_ID.test(continuation.grantId)) {
          fail(`${continuationAt}.grantId is not a safe path component`);
        }
        if (!Number.isSafeInteger(continuation.level)
          || continuation.level < 1) fail(`${continuationAt}.level is invalid`);
        if (!Array.isArray(continuation.nodeIds)
          || continuation.nodeIds.length === 0
          || continuation.nodeIds.some((nodeId, nodeIndex) =>
            typeof nodeId !== 'string' || !FEATURE_ID.test(nodeId)
            || (nodeIndex > 0
              && continuation.nodeIds[nodeIndex - 1]!.localeCompare(nodeId) >= 0))) {
          fail(`${continuationAt}.nodeIds must be sorted unique feature ids`);
        }
        if (!Number.isSafeInteger(continuation.repairs)
          || continuation.repairs < 1) fail(`${continuationAt}.repairs is invalid`);
        if (!HASH.test(continuation.stateSha256 ?? '')) {
          fail(`${continuationAt}.stateSha256 is invalid`);
        }
        const expectedResume = `continuations/${attempt.plan.id}/${continuation.grantId}`;
        if (continuation.resumeFrom !== expectedResume) {
          fail(`${continuationAt}.resumeFrom is not the exact grant workspace`);
        }
        if (Date.parse(continuation.scheduledAt) < Date.parse(execution.completedAt!)) {
          fail(`${continuationAt}.scheduledAt precedes execution completion`);
        }
        if (Date.parse(continuation.scheduledAt) > Date.parse(state.updatedAt)) {
          fail(`${continuationAt}.scheduledAt is newer than campaign state`);
        }
      }
      const eventAt = execution.completedAt ?? execution.startedAt;
      if (Date.parse(eventAt) > Date.parse(state.updatedAt)) fail(`${executionAt} is newer than campaign updatedAt`);
    }
    const latest = attempt.executions.at(-1) ?? null;
    if (attempt.status === 'pending' && latest !== null && latest.status !== 'invalid'
      && !(latest.status === 'completed' && latest.continuation !== undefined)) {
      fail(`${at} is pending without an invalid retryable execution`);
    }
    if (attempt.status === 'running' && latest?.status !== 'running') fail(`${at} has no running execution`);
    if (attempt.status === 'completed' && latest?.status !== 'completed') fail(`${at} has no completed execution`);
    if (attempt.status === 'invalid' && latest?.status !== 'invalid') fail(`${at} has no invalid execution`);
  }
  if (running > state.maxParallel) fail('running executions exceed maxParallel');
  const expected = recalculate(structuredClone(state), state.updatedAt);
  if (expected.status !== state.status
    || Object.entries(expected.summary).some(([key, value]) => state.summary[key] !== value)) {
    fail('summary or status does not match attempts');
  }
  return state;
}

export function createCampaignState(plan: CompiledCampaignPlan,
  { now = new Date().toISOString() }: { now?: string } = {}): CampaignState {
  const identity = campaignIdentity(plan);
  const state: CampaignState = {
    schemaVersion: CAMPAIGN_STATE_SCHEMA_VERSION,
    campaignId: plan.id,
    campaignSha256: identity.sha256,
    status: 'prepared',
    createdAt: now,
    updatedAt: now,
    maxParallel: plan.summary.parallelism,
    attempts: plan.attempts.map(attempt => ({ plan: structuredClone(attempt),
      status: 'pending', executions: [] })),
    summary: { total: 0, pending: 0, running: 0, completed: 0, invalid: 0,
      executions: 0 },
  };
  return recalculate(state, now);
}

export function claimNextAttempt(input: CampaignState,
  { now = new Date().toISOString(), admissionId, runIndices }:
  { now?: string; admissionId?: string; runIndices?: number[] } = {}): {
    state: CampaignState;
    claim: CampaignClaim | null;
    capacityFull: boolean;
  } {
  const state = structuredClone(input);
  const exactAdmissionId = string(admissionId, 'admissionId');
  const availableSlots = runIndices
    ?? Array.from({ length: state.maxParallel }, (_, index) => index);
  if (availableSlots.length !== state.maxParallel
    || new Set(availableSlots).size !== availableSlots.length
    || availableSlots.some(index => !Number.isInteger(index) || index < 0
      || index > RUN_INDEX_CAP)) fail('runIndices must contain one unique valid slot per worker');
  const usedSlots = new Set(state.attempts.flatMap(attempt => attempt.executions
    .filter(execution => execution.status === 'running').map(execution => execution.runIndex)));
  const runIndex = availableSlots.find(index => !usedSlots.has(index));
  if (runIndex === undefined) return { state, claim: null, capacityFull: true };
  const attempt = state.attempts.find(candidate => candidate.status === 'pending');
  if (!attempt) return { state, claim: null, capacityFull: false };
  const previous = attempt.executions.at(-1) ?? null;
  const resumeFrom = previous?.continuation?.resumeFrom
    ?? (previous?.retry?.scheduled === true ? previous.output : null);
  const priorOutputs = attempt.executions.map(execution => execution.output);
  const ordinal = attempt.executions.length + 1;
  const id = `${attempt.plan.id}-execution${ordinal}`;
  const output = `attempts/${attempt.plan.id}/execution-${ordinal}`;
  attempt.status = 'running';
  attempt.executions.push({ id, ordinal, status: 'running', output, startedAt: now,
    completedAt: null, exitCode: null, outcome: null, reason: null, retry: null,
    admissionId: exactAdmissionId, runIndex });
  return { state: recalculate(state, now),
    claim: { attempt: structuredClone(attempt.plan), executionId: id, output, runIndex,
      resumeFrom, priorOutputs, extension: structuredClone(attempt.extension ?? null) },
    capacityFull: false };
}

export function addCampaignExtensions(input: unknown,
  extensions: Readonly<Record<string, CampaignExtensionSeed>>,
  { now = new Date().toISOString() }: { now?: string } = {}): CampaignState {
  const state = validateCampaignState(input);
  if (state.attempts.some(attempt => attempt.executions.length > 0)) {
    throw new Error('campaign extensions must be attached before execution starts');
  }
  for (const attempt of state.attempts) {
    const extension = extensions[attempt.plan.id];
    if (!extension) throw new Error(`campaign attempt ${attempt.plan.id} has no extension source`);
    attempt.extension = structuredClone(extension);
  }
  return recalculate(state, now);
}

export function scheduleDependencyContinuation(input: unknown, attemptId: string,
  continuation: Omit<CampaignContinuation, 'scheduledAt'>,
  { now = new Date().toISOString() }: { now?: string } = {}): CampaignState {
  const state = validateCampaignState(input);
  string(attemptId, 'attemptId');
  if (state.status !== 'completed') {
    throw new Error('dependency continuation requires a completed campaign');
  }
  const attempt = state.attempts.find(candidate => candidate.plan.id === attemptId);
  if (!attempt) throw new Error(`campaign attempt ${attemptId} does not exist`);
  if (attempt.plan.mode?.id !== 'dependency' || attempt.status !== 'completed') {
    throw new Error(`campaign attempt ${attemptId} is not a completed dependency attempt`);
  }
  const execution = attempt.executions.at(-1);
  if (execution?.status !== 'completed' || execution.continuation !== undefined) {
    throw new Error(`campaign attempt ${attemptId} has no unextended completed execution`);
  }
  execution.continuation = { ...structuredClone(continuation),
    nodeIds: [...(continuation?.nodeIds ?? [])].sort(), scheduledAt: now };
  attempt.status = 'pending';
  return recalculate(state, now);
}

export function classifyCampaignExecution({ exitCode = null, timedOut = false, run = null }:
  CampaignExecutionResult = {}): {
    status: Exclude<ExecutionStatus, 'running'>;
    outcome: CampaignOutcome;
    reason: string | null;
  } {
  if (timedOut) return { status: 'invalid', outcome: 'timed_out', reason: 'attempt deadline expired' };
  if (run?.outcome?.kind === 'scheduler_interrupted') {
    return { status: 'invalid', outcome: 'scheduler_interrupted',
      reason: run.outcome?.reason ?? 'scheduler was interrupted' };
  }
  const observedOutcome = run?.outcome?.kind;
  if (exitCode !== 0
    && (observedOutcome === 'provider_failure' || observedOutcome === 'harness_failure')) {
    const outcome = observedOutcome;
    return { status: 'invalid', outcome,
      reason: `attempt process exited ${exitCode ?? 'without a code'}: `
        + `${run?.outcome?.reason ?? `${outcome.replace('_', ' ')} occurred`}` };
  }
  if (run?.contaminated === true) return { status: 'invalid', outcome: 'contaminated',
    reason: run.contamination?.verdict ?? 'run was contaminated' };
  if (exitCode !== 0) return { status: 'invalid', outcome: 'harness_failure',
    reason: `attempt process exited ${exitCode ?? 'without a code'}` };
  if (!run) return { status: 'invalid', outcome: 'missing_artifact',
    reason: `${ARTIFACT_FILE.run} was not produced` };
  const outcome = run.outcome?.kind ?? 'ungraded';
  if (TERMINAL_OUTCOMES.has(outcome)) {
    return { status: 'completed', outcome: outcome as TerminalOutcome, reason: null };
  }
  if (INVALID_OUTCOMES.has(outcome)) return { status: 'invalid', outcome: outcome as InvalidOutcome,
    reason: run.outcome?.reason ?? (outcome === 'inconclusive'
      ? 'one or more selected checks did not produce a pass-or-fail result'
      : outcome === 'incomplete' ? 'grading stopped before every selected check was measured'
        : `run outcome was ${outcome}`) };
  return { status: 'invalid', outcome: 'ungraded',
    reason: `unknown run outcome ${outcome}; exit code ${exitCode ?? 'missing'}` };
}

export function finishCampaignExecution(input: CampaignState, executionId: string,
  result: CampaignExecutionResult,
  { retryOn = [], retries = 0, now = new Date().toISOString() }:
  { retryOn?: string[]; retries?: number; now?: string } = {}): CampaignState {
  const state = structuredClone(input);
  const attempt = state.attempts.find(candidate => candidate.executions.at(-1)?.id === executionId);
  if (!attempt || attempt.status !== 'running') throw new Error(`execution ${executionId} is not running`);
  const execution = attempt.executions.at(-1);
  if (!execution) throw new Error(`execution ${executionId} is not running`);
  const classified = classifyCampaignExecution(result);
  execution.status = classified.status;
  execution.completedAt = now;
  execution.exitCode = result.exitCode ?? null;
  execution.outcome = classified.outcome;
  execution.reason = classified.reason;
  if (classified.status === 'completed') attempt.status = 'completed';
  else {
    const authority = result.retryAuthority ?? {};
    const requested = retryOn.includes(classified.outcome);
    const transient = authority.transient === true;
    const recoveryClean = authority.recoveryClean === true;
    const budgetKnown = authority.budgetKnown === true;
    const budgetAvailable = attempt.executions.length <= retries;
    const scheduled = requested && transient && recoveryClean && budgetKnown && budgetAvailable;
    execution.retry = {
      requested, transient, recoveryClean, budgetKnown, scheduled,
      cause: typeof authority.cause === 'string' && authority.cause ? authority.cause : null,
      reason: !requested ? 'outcome is not configured for retry'
        : !transient ? 'failure is not explicitly transient'
          : !recoveryClean ? 'clean recovery was not proven'
            : !budgetKnown ? 'prior provider spend is unknown'
            : !budgetAvailable ? 'retry budget is exhausted' : 'transient failure has clean recovery proof',
    };
    attempt.status = scheduled ? 'pending' : 'invalid';
  }
  return recalculate(state, now);
}

export function markInterruptedExecution(input: CampaignState, executionId: string,
  { now = new Date().toISOString(), reason = 'scheduler process ended before recording completion',
    retryOn = [] }: { now?: string; reason?: string; retryOn?: string[] } = {}): CampaignState {
  const state = structuredClone(input);
  const attempt = state.attempts.find(candidate => candidate.executions.at(-1)?.id === executionId);
  if (!attempt || attempt.status !== 'running') throw new Error(`execution ${executionId} is not running`);
  const execution = attempt.executions.at(-1);
  if (!execution) throw new Error(`execution ${executionId} is not running`);
  execution.status = 'invalid';
  execution.completedAt = now;
  execution.exitCode = null;
  execution.outcome = 'scheduler_interrupted';
  execution.reason = string(reason, 'interruption reason');
  execution.retry = { requested: retryOn.includes(execution.outcome), transient: false,
    recoveryClean: false, budgetKnown: false, scheduled: false, cause: null,
    reason: 'operator reconciliation is required after scheduler interruption' };
  attempt.status = 'invalid';
  return recalculate(state, now);
}

export interface CampaignPaths {
  root: string;
  plan: string;
  state: string;
}

function paths(directory: string): CampaignPaths {
  const root = resolve(directory);
  return { root, plan: join(root, CAMPAIGN_FILE.plan), state: join(root, CAMPAIGN_FILE.state) };
}

function identities(plan: CompiledCampaignPlan): ReturnType<typeof emptyArtifactIdentities> {
  return emptyArtifactIdentities({ experiment: campaignIdentity(plan) });
}

function assertStateMatchesPlan(state: CampaignState, plan: CompiledCampaignPlan): CampaignState {
  if (state.campaignId !== plan.id || state.campaignSha256 !== plan.contentSha256) {
    throw new Error('campaign state belongs to a different campaign identity');
  }
  const planned = canonicalDefinitionJson(plan.attempts);
  const materialized = canonicalDefinitionJson(state.attempts.map(attempt => attempt.plan));
  if (planned !== materialized) throw new Error('campaign state attempt plan does not match the compiled campaign');
  return state;
}

export interface CampaignDirectory {
  paths: CampaignPaths;
  plan: CompiledCampaignPlan;
  state: CampaignState;
}

export function initializeCampaignDirectory(input: unknown, directory: string,
  options: { now?: string } = {}): CampaignDirectory {
  const plan = validateCompiledCampaignPlan(input);
  const target = paths(directory);
  mkdirSync(target.root, { recursive: true });
  if (existsSync(target.plan)) {
    const existingPlan = validateCompiledCampaignPlan(
      readArtifact(target.plan, { expectedKind: 'campaign_plan' }).payload);
    if (existingPlan.contentSha256 !== plan.contentSha256) {
      throw new Error('campaign directory belongs to a different campaign identity');
    }
    if (!existsSync(target.state)) {
      const recovered = createCampaignState(existingPlan, options);
      writeCampaignState(target.state, existingPlan, recovered);
      return { paths: target, plan: existingPlan, state: recovered };
    }
    const existingState = readArtifact(target.state, { expectedKind: 'campaign_state' }).payload;
    return { paths: target, plan: existingPlan,
      state: assertStateMatchesPlan(validateCampaignState(existingState), existingPlan) };
  }
  if (existsSync(target.state)) {
    throw new Error('campaign state exists without its compiled plan; refusing to guess its identity');
  }
  const state = createCampaignState(plan, options);
  writeArtifact(target.plan, { kind: 'campaign_plan', id: `${plan.id}-plan`,
    identities: identities(plan), payload: plan });
  writeCampaignState(target.state, plan, state);
  return { paths: target, plan, state };
}

export function writeCampaignState(path: string, input: unknown, state: unknown): unknown {
  const plan = validateCompiledCampaignPlan(input);
  return writeArtifact(path, { kind: 'campaign_state', id: `${plan.id}-state`,
    identities: identities(plan), payload: assertStateMatchesPlan(validateCampaignState(state), plan) });
}

export function readCampaignState(directory: string,
  { requireCurrentInputs = true }: { requireCurrentInputs?: boolean } = {}): CampaignDirectory {
  const target = paths(directory);
  const plan = validateCompiledCampaignPlan(
    readArtifact(target.plan, { expectedKind: 'campaign_plan' }).payload,
    { requireCurrentInputs });
  const state = validateCampaignState(readArtifact(target.state, { expectedKind: 'campaign_state' }).payload);
  return { paths: target, plan, state: assertStateMatchesPlan(state, plan) };
}
