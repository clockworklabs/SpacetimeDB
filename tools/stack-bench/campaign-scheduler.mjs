import { existsSync, mkdirSync } from 'node:fs';
import { join, resolve } from 'node:path';

import { emptyArtifactIdentities, readArtifact, writeArtifact } from './artifacts.mjs';
import { campaignIdentity, validateCompiledCampaignPlan } from './campaign-compiler.mjs';
import { canonicalDefinitionJson } from './definition-plan.mjs';

export const CAMPAIGN_STATE_SCHEMA_VERSION = 1;
const ATTEMPT_STATUSES = new Set(['pending', 'running', 'completed', 'invalid']);
const EXECUTION_STATUSES = new Set(['running', 'completed', 'invalid']);
const CAMPAIGN_STATUSES = new Set(['prepared', 'running', 'completed', 'attention-required']);
const TERMINAL_OUTCOMES = new Set(['passed', 'app_failure']);
const INVALID_OUTCOMES = new Set(['harness_failure', 'inconclusive', 'ungraded', 'contaminated',
  'timed_out', 'missing_artifact', 'scheduler_interrupted']);
const SAFE_ID = /^[a-z0-9][a-z0-9.-]*$/;
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const fail = message => { throw new Error(`invalid campaign state: ${message}`); };

function timestamp(value, at) {
  if (typeof value !== 'string' || Number.isNaN(Date.parse(value))) fail(`${at} must be an ISO timestamp`);
  return value;
}

function string(value, at) {
  if (typeof value !== 'string' || !value) fail(`${at} is required`);
  return value;
}

function recalculate(state, now) {
  const counts = Object.fromEntries([...ATTEMPT_STATUSES].sort().map(status =>
    [status, state.attempts.filter(attempt => attempt.status === status).length]));
  state.summary = { total: state.attempts.length, ...counts,
    executions: state.attempts.reduce((total, attempt) => total + attempt.executions.length, 0) };
  if (counts.running) state.status = 'running';
  else if (counts.invalid) state.status = 'attention-required';
  else if (counts.pending) state.status = 'prepared';
  else state.status = 'completed';
  state.updatedAt = now;
  return state;
}

export function validateCampaignState(input) {
  if (!object(input)) fail('document must be an object');
  const state = structuredClone(input);
  const fields = new Set(['schemaVersion', 'campaignId', 'campaignSha256', 'status',
    'createdAt', 'updatedAt', 'attempts', 'summary']);
  for (const key of Object.keys(state)) if (!fields.has(key)) fail(`${key} is unknown`);
  if (state.schemaVersion !== CAMPAIGN_STATE_SCHEMA_VERSION) fail('schemaVersion is unsupported');
  string(state.campaignId, 'campaignId');
  if (!/^[a-f0-9]{64}$/.test(state.campaignSha256)) fail('campaignSha256 is invalid');
  if (!CAMPAIGN_STATUSES.has(state.status)) fail(`status ${state.status} is invalid`);
  timestamp(state.createdAt, 'createdAt');
  timestamp(state.updatedAt, 'updatedAt');
  if (Date.parse(state.updatedAt) < Date.parse(state.createdAt)) fail('updatedAt precedes createdAt');
  if (!Array.isArray(state.attempts) || state.attempts.length === 0) fail('attempts must be non-empty');
  const ids = new Set();
  let running = 0;
  for (const [index, attempt] of state.attempts.entries()) {
    const at = `attempts[${index}]`;
    if (!object(attempt)) fail(`${at} must be an object`);
    const allowed = new Set(['plan', 'status', 'executions']);
    for (const key of Object.keys(attempt)) if (!allowed.has(key)) fail(`${at}.${key} is unknown`);
    if (!object(attempt.plan)) fail(`${at}.plan must be an object`);
    string(attempt.plan.id, `${at}.plan.id`);
    if (!SAFE_ID.test(attempt.plan.id)) fail(`${at}.plan.id is not a safe path component`);
    if (ids.has(attempt.plan.id)) fail(`${at}.plan.id duplicates ${attempt.plan.id}`);
    ids.add(attempt.plan.id);
    if (!ATTEMPT_STATUSES.has(attempt.status)) fail(`${at}.status is invalid`);
    if (!Array.isArray(attempt.executions)) fail(`${at}.executions must be an array`);
    let previousCompletedAt = null;
    for (const [executionIndex, execution] of attempt.executions.entries()) {
      const executionAt = `${at}.executions[${executionIndex}]`;
      const executionFields = new Set(['id', 'ordinal', 'status', 'output', 'startedAt',
        'completedAt', 'exitCode', 'outcome', 'reason', 'admissionId']);
      if (!object(execution)) fail(`${executionAt} must be an object`);
      for (const key of Object.keys(execution)) if (!executionFields.has(key)) fail(`${executionAt}.${key} is unknown`);
      string(execution.id, `${executionAt}.id`);
      string(execution.admissionId, `${executionAt}.admissionId`);
      if (execution.ordinal !== executionIndex + 1) fail(`${executionAt}.ordinal is not contiguous`);
      if (execution.id !== `${attempt.plan.id}-execution${execution.ordinal}`) {
        fail(`${executionAt}.id does not match its attempt and ordinal`);
      }
      if (!EXECUTION_STATUSES.has(execution.status)) fail(`${executionAt}.status is invalid`);
      if (executionIndex < attempt.executions.length - 1 && execution.status !== 'invalid') {
        fail(`${executionAt} is historical but not invalid`);
      }
      string(execution.output, `${executionAt}.output`);
      if (execution.output !== `attempts/${attempt.plan.id}/execution-${execution.ordinal}`) {
        fail(`${executionAt}.output is not the exact execution directory`);
      }
      timestamp(execution.startedAt, `${executionAt}.startedAt`);
      if (Date.parse(execution.startedAt) < Date.parse(state.createdAt)) {
        fail(`${executionAt}.startedAt precedes campaign creation`);
      }
      if (previousCompletedAt && Date.parse(execution.startedAt) < Date.parse(previousCompletedAt)) {
        fail(`${executionAt}.startedAt overlaps the previous execution`);
      }
      if (execution.status === 'running') {
        running += 1;
        if (execution.completedAt !== null || execution.exitCode !== null
          || execution.outcome !== null || execution.reason !== null) fail(`${executionAt} running fields are inconsistent`);
      } else {
        timestamp(execution.completedAt, `${executionAt}.completedAt`);
        if (Date.parse(execution.completedAt) < Date.parse(execution.startedAt)) {
          fail(`${executionAt}.completedAt precedes startedAt`);
        }
        previousCompletedAt = execution.completedAt;
        if (execution.exitCode !== null && !Number.isInteger(execution.exitCode)) fail(`${executionAt}.exitCode is invalid`);
        string(execution.outcome, `${executionAt}.outcome`);
        if (execution.status === 'completed'
          && (execution.exitCode !== 0 || execution.reason !== null)) fail(`${executionAt} completed fields are inconsistent`);
        if (execution.status === 'completed' && !TERMINAL_OUTCOMES.has(execution.outcome)) {
          fail(`${executionAt}.outcome is not terminal`);
        }
        if (execution.status === 'invalid') {
          string(execution.reason, `${executionAt}.reason`);
          if (!INVALID_OUTCOMES.has(execution.outcome)) fail(`${executionAt}.outcome is not invalid`);
        }
      }
      const eventAt = execution.completedAt ?? execution.startedAt;
      if (Date.parse(eventAt) > Date.parse(state.updatedAt)) fail(`${executionAt} is newer than campaign updatedAt`);
    }
    const latest = attempt.executions.at(-1) ?? null;
    if (attempt.status === 'pending' && latest !== null && latest.status !== 'invalid') {
      fail(`${at} is pending without an invalid retryable execution`);
    }
    if (attempt.status === 'running' && latest?.status !== 'running') fail(`${at} has no running execution`);
    if (attempt.status === 'completed' && latest?.status !== 'completed') fail(`${at} has no completed execution`);
    if (attempt.status === 'invalid' && latest?.status !== 'invalid') fail(`${at} has no invalid execution`);
  }
  if (running > 1) fail('more than one execution is running');
  if (!object(state.summary)) fail('summary must be an object');
  const expected = recalculate(structuredClone(state), state.updatedAt);
  if (JSON.stringify(expected.summary) !== JSON.stringify(state.summary) || expected.status !== state.status) {
    fail('summary or status does not match attempts');
  }
  return state;
}

export function createCampaignState(plan, { now = new Date().toISOString() } = {}) {
  plan = validateCompiledCampaignPlan(plan);
  const identity = campaignIdentity(plan);
  const state = {
    schemaVersion: CAMPAIGN_STATE_SCHEMA_VERSION,
    campaignId: plan.id,
    campaignSha256: identity.sha256,
    status: 'prepared',
    createdAt: now,
    updatedAt: now,
    attempts: plan.attempts.map(attempt => ({ plan: structuredClone(attempt),
      status: 'pending', executions: [] })),
    summary: {},
  };
  return validateCampaignState(recalculate(state, now));
}

export function claimNextAttempt(input, { now = new Date().toISOString(), admissionId } = {}) {
  const state = validateCampaignState(input);
  if (state.attempts.some(attempt => attempt.status === 'running')) {
    throw new Error('campaign already has a running attempt');
  }
  string(admissionId, 'admissionId');
  const attempt = state.attempts.find(candidate => candidate.status === 'pending');
  if (!attempt) return { state, claim: null };
  const ordinal = attempt.executions.length + 1;
  const id = `${attempt.plan.id}-execution${ordinal}`;
  const output = `attempts/${attempt.plan.id}/execution-${ordinal}`;
  attempt.status = 'running';
  attempt.executions.push({ id, ordinal, status: 'running', output, startedAt: now,
    completedAt: null, exitCode: null, outcome: null, reason: null, admissionId });
  return { state: validateCampaignState(recalculate(state, now)),
    claim: { attempt: structuredClone(attempt.plan), executionId: id, output } };
}

export function classifyCampaignExecution({ exitCode = null, timedOut = false, run = null } = {}) {
  if (timedOut) return { status: 'invalid', outcome: 'timed_out', reason: 'attempt deadline expired' };
  if (exitCode !== 0 && run?.outcome?.kind === 'harness_failure') {
    return { status: 'invalid', outcome: 'harness_failure',
      reason: `attempt process exited ${exitCode ?? 'without a code'}: ${run.outcome.reason ?? 'harness failed'}` };
  }
  if (exitCode !== 0) return { status: 'invalid', outcome: 'harness_failure',
    reason: `attempt process exited ${exitCode ?? 'without a code'}` };
  if (!run) return { status: 'invalid', outcome: 'missing_artifact', reason: 'run.json was not produced' };
  if (run.contaminated === true) return { status: 'invalid', outcome: 'contaminated',
    reason: run.contamination?.verdict ?? 'run was contaminated' };
  const outcome = run.outcome?.kind ?? 'ungraded';
  if (outcome === 'scheduler_interrupted') return { status: 'invalid', outcome,
    reason: run.outcome?.reason ?? 'scheduler was interrupted' };
  if (TERMINAL_OUTCOMES.has(outcome)) return { status: 'completed', outcome, reason: null };
  if (INVALID_OUTCOMES.has(outcome)) return { status: 'invalid', outcome,
    reason: run.outcome?.reason ?? (outcome === 'inconclusive'
      ? 'one or more selected checks did not produce a pass-or-fail result'
      : `run outcome was ${outcome}`) };
  return { status: 'invalid', outcome: 'ungraded',
    reason: `unknown run outcome ${outcome}; exit code ${exitCode ?? 'missing'}` };
}

export function finishCampaignExecution(input, executionId, result,
  { retryOn = [], retries = 0, now = new Date().toISOString() } = {}) {
  const state = validateCampaignState(input);
  const attempt = state.attempts.find(candidate => candidate.executions.at(-1)?.id === executionId);
  if (!attempt || attempt.status !== 'running') throw new Error(`execution ${executionId} is not running`);
  const execution = attempt.executions.at(-1);
  const classified = classifyCampaignExecution(result);
  execution.status = classified.status;
  execution.completedAt = now;
  execution.exitCode = result.exitCode ?? null;
  execution.outcome = classified.outcome;
  execution.reason = classified.reason;
  if (classified.status === 'completed') attempt.status = 'completed';
  else if (retryOn.includes(classified.outcome) && attempt.executions.length <= retries) attempt.status = 'pending';
  else attempt.status = 'invalid';
  return validateCampaignState(recalculate(state, now));
}

export function markInterruptedExecution(input, executionId,
  { now = new Date().toISOString(), reason = 'scheduler process ended before recording completion',
    retryOn = [], retries = 0 } = {}) {
  const state = validateCampaignState(input);
  const attempt = state.attempts.find(candidate => candidate.executions.at(-1)?.id === executionId);
  if (!attempt || attempt.status !== 'running') throw new Error(`execution ${executionId} is not running`);
  const execution = attempt.executions.at(-1);
  execution.status = 'invalid';
  execution.completedAt = now;
  execution.exitCode = null;
  execution.outcome = 'scheduler_interrupted';
  execution.reason = string(reason, 'interruption reason');
  if (retryOn.includes(execution.outcome) && attempt.executions.length <= retries) attempt.status = 'pending';
  else attempt.status = 'invalid';
  return validateCampaignState(recalculate(state, now));
}

function paths(directory) {
  const root = resolve(directory);
  return { root, plan: join(root, 'plan.json'), state: join(root, 'state.json') };
}

function identities(plan) {
  return emptyArtifactIdentities({ experiment: campaignIdentity(plan) });
}

function assertStateMatchesPlan(state, plan) {
  if (state.campaignId !== plan.id || state.campaignSha256 !== plan.contentSha256) {
    throw new Error('campaign state belongs to a different campaign identity');
  }
  const planned = canonicalDefinitionJson(plan.attempts);
  const materialized = canonicalDefinitionJson(state.attempts.map(attempt => attempt.plan));
  if (planned !== materialized) throw new Error('campaign state attempt plan does not match the compiled campaign');
  return state;
}

export function initializeCampaignDirectory(plan, directory, options = {}) {
  plan = validateCompiledCampaignPlan(plan);
  const target = paths(directory);
  mkdirSync(target.root, { recursive: true });
  if (existsSync(target.plan)) {
    const existingPlan = validateCompiledCampaignPlan(
      readArtifact(target.plan, { expectedKind: 'campaign_plan' }).payload);
    if (canonicalDefinitionJson(existingPlan) !== canonicalDefinitionJson(plan)) {
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

export function writeCampaignState(path, plan, state) {
  plan = validateCompiledCampaignPlan(plan);
  return writeArtifact(path, { kind: 'campaign_state', id: `${plan.id}-state`,
    identities: identities(plan), payload: assertStateMatchesPlan(validateCampaignState(state), plan) });
}

export function readCampaignState(directory) {
  const target = paths(directory);
  const plan = validateCompiledCampaignPlan(readArtifact(target.plan, { expectedKind: 'campaign_plan' }).payload);
  const state = validateCampaignState(readArtifact(target.state, { expectedKind: 'campaign_state' }).payload);
  return { paths: target, plan, state: assertStateMatchesPlan(state, plan) };
}
