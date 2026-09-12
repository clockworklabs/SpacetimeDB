import { randomUUID } from 'node:crypto';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { isAbsolute, join } from 'node:path';
import { z } from 'zod';
import { campaignChildPath } from './campaign-path.js';
import { campaignCancellationRequested, campaignLockIsActive, readCampaignLock,
  writeCampaignRecord as write } from './campaign-lock.js';
import { readCampaignState } from './campaign-scheduler.js';
import { readCampaignTimeBudget } from './campaign-time-grant.js';
import { sleepSync } from '../runtime/platform.js';

const ENV = 'STACK_BENCH_PROVIDER_WAIT_CONTEXT';
const id = z.string().regex(/^[a-zA-Z0-9][a-zA-Z0-9.-]*$/);
const contextSchema = z.object({ directory: z.string().refine(isAbsolute),
  campaignSha256: z.string(), attemptId: id, executionId: id,
  ownershipMarkerSha256: z.string(), root: z.string().refine(isAbsolute) });
type Context = z.infer<typeof contextSchema>;
const waitSchema = contextSchema.extend({ generation: z.string().uuid(), createdAt: z.string(),
  heartbeatAt: z.string(), disposition: z.enum(['waiting', 'continued', 'ended']),
  evidence: z.record(z.string(), z.unknown()), reason: z.string().optional(), endedAt: z.string().optional() });
type Wait = z.infer<typeof waitSchema>;
const requestSchema = contextSchema.extend({ generation: z.string().uuid(), requestId: id,
  requestedAt: z.string() });
type Request = z.infer<typeof requestSchema>;

function read(path: string): unknown { return JSON.parse(readFileSync(path, 'utf8')); }

function activeContext(directory: string, attemptId: string): Context {
  const { plan, state } = readCampaignState(directory, { requireCurrentInputs: false });
  const attempt = state.attempts.find(value => value.plan.id === attemptId);
  const execution = attempt?.executions.at(-1);
  if (attempt?.status !== 'running' || execution?.status !== 'running') {
    throw new Error('only a live running execution can continue; stopped executions cannot be restored');
  }
  const budget = readCampaignTimeBudget(directory, attemptId);
  if (budget.consumedMs >= budget.effectiveMinutes * 60_000) {
    throw new Error('provider continuation duration allowance is exhausted');
  }
  const lock = readCampaignLock(directory);
  if (!lock || !campaignLockIsActive(directory, plan)
    || campaignCancellationRequested({ path: join(directory, '.campaign.lock.json'), record: lock, token: '' })) {
    throw new Error('provider continuation requires an active uncancelled controller owner');
  }
  return { directory, campaignSha256: plan.contentSha256, attemptId, executionId: execution.id,
    ownershipMarkerSha256: lock.ownershipMarkerSha256,
    root: campaignChildPath(directory, join(execution.output, 'provider-waits'), 'provider wait directory') };
}
function sameContext(a: Context, b: Context): boolean {
  return Object.keys(contextSchema.shape).every(key => a[key as keyof Context] === b[key as keyof Context]);
}
function assertActive(context: Context): void {
  if (!sameContext(context, activeContext(context.directory, context.attemptId))) {
    throw new Error('provider continuation execution or controller ownership changed');
  }
}

export function campaignProviderContinuationContext(env: NodeJS.ProcessEnv = process.env): Context | null {
  if (!env[ENV]) return null;
  const context = contextSchema.parse(JSON.parse(env[ENV]));
  assertActive(context);
  return context;
}

export function persistCampaignProviderInvocation({ env = process.env, evidence }:
  { env?: NodeJS.ProcessEnv; evidence: Record<string, unknown> }): void {
  if (!env[ENV]) return;
  const context = contextSchema.parse(JSON.parse(env[ENV]));
  // A settled invocation must retain its receipt even after cancellation or owner exit.
  // Only starting/waiting for more work requires a live controller.
  const { plan, state } = readCampaignState(context.directory, { requireCurrentInputs: false });
  const execution = state.attempts.find(attempt => attempt.plan.id === context.attemptId)
    ?.executions.find(execution => execution.id === context.executionId);
  if (!execution || context.campaignSha256 !== plan.contentSha256
    || context.root !== campaignChildPath(context.directory,
      join(execution.output, 'provider-waits'), 'provider receipt directory')) {
    throw new Error('provider receipt does not belong to its stored campaign execution');
  }
  const actionId = id.parse(evidence.actionId);
  const invocation = z.number().int().positive().parse(evidence.invocation);
  write(join(context.root, 'invocations', `${actionId}-${invocation}.json`),
    { ...context, recordedAt: new Date().toISOString(), evidence }, true);
}

export interface ProviderContinuationStatus {
  eligible: boolean; waiting: boolean; reason: string | null; wait?: Wait;
}

export interface CampaignProviderWaitHistory {
  waits: number; waitedMs: number; continued: number; stopped: number; waiting: number;
  durationKind: 'exact' | 'lower-bound';
  details: Array<{ generation: string; disposition: 'continued' | 'stopped' | 'waiting';
    waitedMs: number; durationKind: 'exact' | 'lower-bound' }>;
}

/** Interrupted processes may have no final agent result. Immutable wait events remain evidence. */
export function readCampaignProviderWaitHistory(directory: string, attemptId: string,
  executionId: string): CampaignProviderWaitHistory | null {
  const { plan, state } = readCampaignState(directory, { requireCurrentInputs: false });
  const execution = state.attempts.find(attempt => attempt.plan.id === attemptId)
    ?.executions.find(execution => execution.id === executionId);
  if (!execution) throw new Error('unknown provider wait execution');
  const root = campaignChildPath(directory, join(execution.output, 'provider-waits'), 'provider wait history');
  const events = join(root, 'events');
  if (!existsSync(events)) return null;
  const eventNames = readdirSync(events);
  const names = eventNames.filter(name => name.endsWith('.waiting.json')).sort();
  if (eventNames.some(name => /\.(accepted|ended)\.json$/.test(name)
    && !names.includes(name.replace(/\.(accepted|ended)\.json$/, '.waiting.json')))) {
    throw new Error('provider wait disposition has no matching wait event');
  }
  if (!names.length) return null;
  const currentPath = join(root, 'current.json');
  const current = existsSync(currentPath) ? waitSchema.parse(read(currentPath)) : null;
  let owner: Context | null = null;
  const details: CampaignProviderWaitHistory['details'] = names.map(name => {
    const wait = waitSchema.parse(read(join(events, name)));
    if (name !== `${wait.generation}.waiting.json` || wait.disposition !== 'waiting'
      || wait.campaignSha256 !== plan.contentSha256 || wait.attemptId !== attemptId
      || wait.executionId !== executionId || wait.root.replaceAll('\\', '/')
        !== join(wait.directory, execution.output, 'provider-waits').replaceAll('\\', '/')
      || (owner && !sameContext(owner, wait))) throw new Error('provider wait event binding is invalid');
    owner ??= wait;
    const acceptedPath = join(events, `${wait.generation}.accepted.json`);
    const endedPath = join(events, `${wait.generation}.ended.json`);
    let end = wait.heartbeatAt;
    let disposition: 'continued' | 'stopped' | 'waiting' = 'stopped';
    let durationKind: 'exact' | 'lower-bound' = 'lower-bound';
    if (existsSync(acceptedPath)) {
      const accepted = requestSchema.extend({ acceptedAt: z.string() }).parse(read(acceptedPath));
      if (!sameContext(wait, accepted) || accepted.generation !== wait.generation) {
        throw new Error('provider wait acceptance binding is invalid');
      }
      end = accepted.acceptedAt;
      disposition = 'continued';
      durationKind = 'exact';
    } else if (existsSync(endedPath)) {
      const ended = waitSchema.parse(read(endedPath));
      if (!sameContext(wait, ended) || ended.generation !== wait.generation || ended.disposition !== 'ended') {
        throw new Error('provider wait end binding is invalid');
      }
      end = ended.endedAt ?? ended.heartbeatAt;
      durationKind = ended.endedAt ? 'exact' : 'lower-bound';
    } else if (current?.generation === wait.generation) {
      if (!sameContext(wait, current)) throw new Error('provider wait heartbeat binding is invalid');
      end = current.heartbeatAt;
      const age = Date.now() - Date.parse(end);
      if (execution.status === 'running' && current.disposition === 'waiting'
        && age >= -5_000 && age <= 30_000) disposition = 'waiting';
    }
    const waitedMs = Date.parse(end) - Date.parse(wait.createdAt);
    if (!Number.isSafeInteger(waitedMs) || waitedMs < 0) throw new Error('provider wait timestamps are invalid');
    return { generation: wait.generation, disposition, waitedMs, durationKind };
  });
  return { waits: details.length, waitedMs: details.reduce((sum, wait) => sum + wait.waitedMs, 0),
    continued: details.filter(wait => wait.disposition === 'continued').length,
    stopped: details.filter(wait => wait.disposition === 'stopped').length,
    waiting: details.filter(wait => wait.disposition === 'waiting').length,
    durationKind: details.some(wait => wait.durationKind === 'lower-bound') ? 'lower-bound' : 'exact', details };
}
export function readCampaignProviderContinuationStatus(directory: string,
  attemptId: string): ProviderContinuationStatus {
  try {
    const context = activeContext(directory, attemptId);
    const current = join(context.root, 'current.json');
    if (!existsSync(current)) return { eligible: false, waiting: false, reason: 'execution is not waiting for its provider' };
    const wait = waitSchema.parse(read(current));
    if (!sameContext(context, wait)) throw new Error('wait belongs to a different execution or controller');
    if (wait.disposition !== 'waiting') return { eligible: false, waiting: false,
      reason: wait.reason ?? `wait ${wait.disposition}`, wait };
    const age = Date.now() - Date.parse(wait.heartbeatAt);
    if (!Number.isFinite(age) || age < -5_000 || age > 30_000) throw new Error('live provider wait heartbeat is stale');
    return { eligible: true, waiting: true, reason: null, wait };
  } catch (error) {
    return { eligible: false, waiting: false, reason: error instanceof Error ? error.message : String(error) };
  }
}

export function requestCampaignProviderContinuation(directory: string,
  { attemptId, requestId }: { attemptId: string; requestId: string }): Request {
  id.parse(attemptId); id.parse(requestId);
  const status = readCampaignProviderContinuationStatus(directory, attemptId);
  const wait = status.wait;
  if (!wait) throw new Error(status.reason ?? 'execution is not waiting');
  const path = join(wait.root, 'requests', `${requestId}.json`);
  const request = { ...contextSchema.parse(wait), generation: wait.generation,
    requestId, requestedAt: new Date().toISOString() };
  const duplicate = (): Request => {
    const prior = requestSchema.parse(read(path));
    if (!sameContext(prior, request) || prior.generation !== request.generation) {
      throw new Error('request id already belongs to a different wait generation');
    }
    return prior;
  };
  if (existsSync(path)) return duplicate();
  if (!status.eligible) throw new Error(status.reason ?? 'execution is not eligible');
  assertActive(wait);
  try { write(path, request, true); }
  catch (error) { if (existsSync(path)) return duplicate(); throw error; }
  return request;
}

export function waitForCampaignProviderContinuation({ env = process.env, evidence, validate,
  sleep = sleepSync }:
  { env?: NodeJS.ProcessEnv; evidence: Record<string, unknown>; validate: (phase: 'waiting' | 'continue') => void;
    sleep?: (ms: number) => void }): { requestId: string; generation: string } | null {
  const context = campaignProviderContinuationContext(env);
  if (!context) return null;
  validate('waiting');
  const actionId = id.parse(evidence.actionId);
  const invocation = z.number().int().positive().parse(evidence.invocation);
  if (!existsSync(join(context.root, 'invocations', `${actionId}-${invocation}.json`))) {
    throw new Error('provider waiting requires its durable invocation receipt');
  }
  const wait: Wait = { ...context, generation: randomUUID(), evidence,
    createdAt: new Date().toISOString(), heartbeatAt: new Date().toISOString(), disposition: 'waiting' };
  write(join(context.root, 'events', `${wait.generation}.waiting.json`), wait, true);
  try {
    while (true) {
      assertActive(context);
      wait.heartbeatAt = new Date().toISOString();
      write(join(context.root, 'current.json'), wait, false);
      const requests = join(context.root, 'requests');
      for (const name of existsSync(requests) ? readdirSync(requests).filter(name => name.endsWith('.json')).sort() : []) {
        const request = requestSchema.parse(read(join(requests, name)));
        if (!sameContext(request, context) || request.generation !== wait.generation) continue;
        assertActive(context);
        validate('continue');
        assertActive(context);
        write(join(context.root, 'events', `${wait.generation}.accepted.json`),
          { ...request, acceptedAt: new Date().toISOString() }, true);
        wait.disposition = 'continued';
        write(join(context.root, 'current.json'), wait, false);
        return { requestId: request.requestId, generation: wait.generation };
      }
      sleep(2_000);
    }
  } catch (error) {
    wait.disposition = 'ended';
    wait.endedAt = new Date().toISOString();
    wait.reason = error instanceof Error ? error.message : String(error);
    write(join(context.root, 'events', `${wait.generation}.ended.json`), wait, true);
    write(join(context.root, 'current.json'), wait, false);
    throw error;
  }
}
