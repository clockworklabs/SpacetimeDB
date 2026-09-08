import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { z } from 'zod';
import { readCampaignState, scheduleTimeContinuation, timeGrantRequestSchema, timeGrantReceiptSchema, writeCampaignState } from './campaign-scheduler.js';
export { timeGrantReceiptSchema } from './campaign-scheduler.js';
import type { CampaignAttemptState } from './campaign-scheduler.js';
import type { CompiledCampaignPlan } from './campaign-compiler.js';
import { campaignProgressionOwner } from './campaign-compiler.js';
import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { ARTIFACT_FILE, readArtifactPayload } from '../evidence/artifacts.js';
import { campaignChildPath } from './campaign-path.js';
import { acquireCampaignLock, campaignLockIsActive, releaseCampaignLock, writeCampaignRecord } from './campaign-lock.js';
import { timeContinuationEligibility } from '../progression/live-progression.js';
import { publicRecoveryProvesCleanup, remainingAttemptCostBudget } from './campaign-runner.js';

const safeId = z.string().regex(/^[a-z0-9][a-z0-9.-]*$/);
const minutes = z.number().int().positive().refine(n => Number.isSafeInteger(n * 60_000));
export type TimeGrantReceipt = z.infer<typeof timeGrantReceiptSchema>;
export interface CampaignTimeBudget {
  originalMinutes: number; effectiveMinutes: number; consumedMs: number;
  extensionCount: number; grants: TimeGrantReceipt[];
  liveGrantSupported?: boolean;
  continuation?: { eligible: boolean; reason?: string };
  observedAt?: string;
}

export function campaignTimeBudget(plan: CompiledCampaignPlan, attempt: CampaignAttemptState,
  now: number | string = Date.now()): CampaignTimeBudget {
  const at = typeof now === 'string' ? Date.parse(now) : now;
  const grants = attempt.timeGrants ?? [];
  const accepted = grants.filter(g => g.disposition === 'accepted');
  const originalMinutes = plan.definition.budgets.attemptTimeoutMinutes;
  const effectiveMinutes = originalMinutes + accepted.reduce((n, g) => n + g.request.minutes, 0);
  if (!Number.isSafeInteger(effectiveMinutes * 60_000)) throw new Error('time allowance overflow');
  const consumedMs = attempt.executions.reduce((total, execution) => {
    const start = Date.parse(execution.startedAt);
    const end = execution.completedAt ? Date.parse(execution.completedAt)
      : execution.status === 'running' ? at : NaN;
    if (!Number.isFinite(start) || !Number.isFinite(end) || end < start) {
      throw new Error(`execution ${execution.id} has unknown consumed time`);
    }
    return total + end - start;
  }, 0);
  return { originalMinutes, effectiveMinutes, consumedMs, extensionCount: accepted.length, grants,
    observedAt: new Date(at).toISOString(),
    liveGrantSupported: attempt.executions.at(-1)?.timeExtensionSupported === true };
}

export function readTimeGrantRequests(directory: string): TimeGrantReceipt['request'][] {
  const path = campaignChildPath(directory, 'time-requests', 'time requests');
  if (!existsSync(path)) return [];
  return readdirSync(path).filter(name => name.endsWith('.json')).sort().map(name =>
    timeGrantRequestSchema.parse(JSON.parse(readFileSync(
      campaignChildPath(directory, join('time-requests', name), 'time request'), 'utf8'))));
}

export function readCampaignTimeBudget(directory: string, attemptId: string): CampaignTimeBudget {
  const { plan, state } = readCampaignState(directory, { requireCurrentInputs: false });
  const attempt = state.attempts.find(a => a.plan.id === attemptId);
  if (!attempt) throw new Error(`unknown attempt ${attemptId}`);
  const budget = campaignTimeBudget(plan, attempt);
  const last = attempt.executions.at(-1);
  if (last?.outcome === 'timed_out') {
    const eligibility = campaignTimeContinuationEligibility(directory, attemptId);
    budget.continuation = eligibility.eligible ? { eligible: true }
      : { eligible: false, reason: eligibility.reason };
  }
  budget.grants = [...budget.grants, ...readTimeGrantRequests(directory)
    .filter(r => r.attemptId === attemptId && !budget.grants.some(g => g.request.grantId === r.grantId))
    .map(request => ({ request, disposition: 'pending' as const }))];
  return budget;
}

export function campaignTimeContinuationEligibility(directory: string, attemptId: string):
  { eligible: true; stateSha256: string } | { eligible: false; reason: string } {
  try {
    const current = readCampaignState(directory);
    if (current.state.attempts.some(a => a.status === 'running')) throw new Error('campaign still has running work');
    const target = current.state.attempts.find(a => a.plan.id === attemptId);
    const last = target?.executions.at(-1);
    if (!target || target.plan.mode?.id !== 'dependency' || last?.outcome !== 'timed_out'
      || last.timeContinuation) throw new Error('only unextended timed-out dependency attempts can continue');
    const output = campaignChildPath(directory, last.output, 'continuation execution');
    const eligibility = timeContinuationEligibility(output);
    if (!eligibility.eligible) return eligibility;
    const saved = readArtifactPayload<{ owner: unknown }>(join(output, ARTIFACT_FILE.progressionState),
      { expectedKind: 'progression_state' });
    if (canonicalDefinitionJson(saved.owner) !== canonicalDefinitionJson(campaignProgressionOwner(
      current.plan, target.plan, { workspace: true }))) throw new Error('continuation belongs to a different attempt');
    if (!publicRecoveryProvesCleanup(output, target.plan.stack, target.plan.id, last.id)) {
      throw new Error('continuation requires verified resource cleanup');
    }
    remainingAttemptCostBudget(current.plan, { attempt: target.plan,
      priorOutputs: target.executions.map(e => e.output) }, directory);
    return { eligible: true, stateSha256: eligibility.stateSha256 };
  } catch (error) {
    return { eligible: false, reason: error instanceof Error ? error.message : String(error) };
  }
}

export function requestCampaignTimeGrant(directory: string,
  input: { attemptId: string; grantId: string; minutes: number }): TimeGrantReceipt {
  safeId.parse(input.attemptId); safeId.parse(input.grantId); minutes.parse(input.minutes);
  const { plan, state } = readCampaignState(directory, { requireCurrentInputs: false });
  const attempt = state.attempts.find(a => a.plan.id === input.attemptId);
  if (!attempt) throw new Error(`unknown attempt ${input.attemptId}`);
  const prior = readCampaignTimeBudget(directory, input.attemptId).grants
    .find(g => g.request.grantId === input.grantId);
  if (prior) {
    if (prior.request.minutes !== input.minutes) throw new Error('grant id already has different content');
    return prior;
  }
  const execution = attempt.executions.at(-1);
  if (attempt.status !== 'running' || execution?.status !== 'running') {
    const lock = acquireCampaignLock(directory, plan);
    try {
      const current = readCampaignState(directory);
      const target = current.state.attempts.find(a => a.plan.id === input.attemptId)!;
      const duplicate = target.timeGrants?.find(g => g.request.grantId === input.grantId);
      if (duplicate) {
        if (duplicate.request.minutes !== input.minutes) throw new Error('grant id already has different content');
        return duplicate;
      }
      const last = target.executions.at(-1);
      if (!last || last.outcome !== 'timed_out') throw new Error('only timed-out attempts can continue with added time');
      const eligibility = campaignTimeContinuationEligibility(directory, input.attemptId);
      if (!eligibility.eligible) throw new Error(eligibility.reason);
      const previousMinutes = campaignTimeBudget(current.plan, target).effectiveMinutes;
      const effectiveMinutes = previousMinutes + input.minutes;
      if (!Number.isSafeInteger(effectiveMinutes * 60_000 + Date.now())) throw new Error('time allowance overflow');
      const acceptedAt = new Date().toISOString();
      const receipt: TimeGrantReceipt = { request: { ...input, campaignSha256: current.plan.contentSha256,
        executionId: last.id, requestedAt: acceptedAt }, disposition: 'accepted', acceptedAt,
        previousMinutes, effectiveMinutes };
      const next = scheduleTimeContinuation(current.state, input.attemptId, receipt, eligibility.stateSha256);
      writeCampaignState(current.paths.state, current.plan, next);
      return receipt;
    } finally { releaseCampaignLock(lock); }
  }
  if (!execution.timeExtensionSupported) throw new Error('this execution controller does not support live time grants');
  if (!campaignLockIsActive(directory, plan)) throw new Error('time grant requires an active controller owner');
  const request = timeGrantRequestSchema.parse({ ...input, campaignSha256: plan.contentSha256,
    executionId: execution.id, requestedAt: new Date().toISOString() });
  const key = createHash('sha256').update(JSON.stringify([input.attemptId, input.grantId])).digest('hex');
  const path = campaignChildPath(directory, join('time-requests', `${key}.json`), 'time request');
  try { writeCampaignRecord(path, request); }
  catch (error) {
    if (!existsSync(path)) throw error;
    const existing = timeGrantRequestSchema.parse(JSON.parse(readFileSync(path, 'utf8')));
    if (existing.minutes !== request.minutes || existing.executionId !== request.executionId
      || existing.campaignSha256 !== request.campaignSha256) throw new Error('grant id already has different content');
    return { request: existing, disposition: 'pending' };
  }
  return { request, disposition: 'pending' };
}
