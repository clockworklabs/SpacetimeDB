import { randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, readdirSync, realpathSync } from 'node:fs';
import { isAbsolute, join, relative, resolve, sep } from 'node:path';
import { z } from 'zod';
import { executionCredentialsSchema, validateExecutionCredentialTargets } from '../agents/credential-profiles.js';
import { compileCampaignFile } from './campaign-compiler.js';
import { writeCampaignRecord } from './campaign-lock.js';
import { executeCampaign } from './campaign-runner.js';
import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { redactCredentials } from '../evidence/diagnostic-sanitizer.js';
import { sha256 } from '../evidence/provenance.js';

const name = z.string().regex(/^[a-z0-9][a-z0-9_.-]{0,127}$/);
const digest = z.string().regex(/^[a-f0-9]{64}$/);
const submissionSchema = z.strictObject({
  key: name, planFile: z.string().min(1),
  credentials: executionCredentialsSchema.default({}),
  hostId: name.optional(), capacityPolicy: z.enum(['wait', 'fail']).default('wait'),
});
const jobSchema = z.strictObject({
  schemaVersion: z.literal(1), id: digest, key: name, submittedAt: z.iso.datetime(),
  planSha256: digest, requestSha256: digest,
  credentials: executionCredentialsSchema,
  hostId: name.optional(), capacityPolicy: z.enum(['wait', 'fail']),
});
const claimSchema = z.strictObject({ hostId: name, token: digest, startedAt: z.iso.datetime() });
const resultSchema = z.strictObject({
  status: z.enum(['completed', 'failed', 'cancelled']), completedAt: z.iso.datetime(),
  hostId: name, token: digest, error: z.string().optional(),
  campaign: z.unknown().optional(),
});
export type ExecutionJob = z.infer<typeof jobSchema>;
const errorCode = (error: unknown) => error instanceof Error && 'code' in error ? error.code : null;
const rootFor = (results: string) => join(resolve(results), 'jobs');
const directoryFor = (results: string, id: string) => join(rootFor(results), digest.parse(id));
const read = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));

function containedPlan(results: string, path: string): string {
  const root = realpathSync(join(resolve(results), 'plans'));
  const absolute = realpathSync(resolve(root, path));
  const child = relative(root, absolute);
  if (!child || isAbsolute(child) || child.startsWith(`..${sep}`) || child === '..') {
    throw new Error('job plan must be a file inside the plans directory');
  }
  return absolute;
}

/** Trusted service/CLI boundary. Authenticate the caller before invoking this function. */
export function submitExecutionJob(results: string, input: unknown): ExecutionJob {
  const request = submissionSchema.parse(input);
  const planFile = containedPlan(results, request.planFile);
  const source = readFileSync(planFile, 'utf8');
  const plan = compileCampaignFile(planFile);
  if (plan.state !== 'frozen' && (plan.state !== 'draft'
    || plan.agents.some(agent => agent.costLimit !== 'non-billable'))) {
    throw new Error('job submission requires a frozen campaign or a model-free draft');
  }
  validateExecutionCredentialTargets(request.credentials,
    plan.agents.map(agent => agent.adapter), plan.attempts.map(attempt => attempt.id));
  const id = sha256(request.key);
  const { planFile: _planFile, ...policy } = request;
  const requestSha256 = sha256(canonicalDefinitionJson({ ...policy, planSha256: plan.contentSha256 }));
  const directory = directoryFor(results, id);
  const path = join(directory, 'job.json');
  if (existsSync(path)) {
    const existing = jobSchema.parse(read(path));
    if (existing.requestSha256 !== requestSha256) throw new Error('job key already has a different submission');
    return existing;
  }
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  // Publish the immutable plan before the job becomes visible to workers.
  const snapshot = join(directory, 'plan.json');
  try { writeCampaignRecord(snapshot, JSON.parse(source)); }
  catch (error) { if (errorCode(error) !== 'EEXIST') throw error; }
  const saved = compileCampaignFile(snapshot);
  if (saved.contentSha256 !== plan.contentSha256) throw new Error('job key has a different plan snapshot');
  const job: ExecutionJob = { schemaVersion: 1, id, key: request.key,
    submittedAt: new Date().toISOString(), planSha256: plan.contentSha256, requestSha256,
    credentials: request.credentials, capacityPolicy: request.capacityPolicy,
    ...(request.hostId ? { hostId: request.hostId } : {}) };
  try { writeCampaignRecord(path, job); }
  catch (error) {
    if (errorCode(error) !== 'EEXIST') throw error;
    const existing = jobSchema.parse(read(path));
    if (existing.requestSha256 !== requestSha256) throw new Error('job key already has a different submission');
    return existing;
  }
  return job;
}

export function readExecutionJob(results: string, id: string) {
  const directory = directoryFor(results, id);
  const job = jobSchema.parse(read(join(directory, 'job.json')));
  if (job.id !== id || sha256(job.key) !== id) throw new Error('job identity does not match its directory');
  const claimPath = join(directory, 'claim.json'), resultPath = join(directory, 'result.json');
  // Results are published after their immutable claim. Read in that order so a
  // worker completing during this read cannot produce a result without its claim.
  const result = existsSync(resultPath) ? resultSchema.parse(read(resultPath)) : null;
  const claim = existsSync(claimPath) ? claimSchema.parse(read(claimPath)) : null;
  if (result && (!claim || result.token !== claim.token || result.hostId !== claim.hostId)) {
    throw new Error('job result does not belong to its worker claim');
  }
  const cancelled = existsSync(join(directory, 'cancel.json'));
  const waitPath = join(directory, 'capacity.json');
  const capacityWait = existsSync(waitPath) ? z.object({ reason: z.string().nullable(), at: z.iso.datetime() }).parse(read(waitPath)) : null;
  return { job, status: result?.status ?? (claim ? 'running' : cancelled ? 'cancelled' : 'queued'),
    hostId: claim?.hostId ?? job.hostId ?? null, startedAt: claim?.startedAt ?? null,
    completedAt: result?.completedAt ?? null, error: result?.error,
    capacityWait: result ? null : capacityWait,
    campaignDirectory: join(resolve(results), 'campaigns', `job-${job.id}`),
    cancellationRequested: cancelled };
}

export function listExecutionJobs(results: string, { after = '', limit = 50 } = {}) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 200) throw new Error('job page size must be 1–200');
  if (after) digest.parse(after);
  const root = rootFor(results);
  if (!existsSync(root)) return { jobs: [], next: null };
  const ids = readdirSync(root).filter(id => digest.safeParse(id).success
    && id > after && existsSync(join(root, id, 'job.json'))).sort();
  const page = ids.slice(0, limit);
  return { jobs: page.map(id => readExecutionJob(results, id)), next: ids.length > limit ? page.at(-1)! : null };
}

export function cancelExecutionJob(results: string, id: string): void {
  if (['completed', 'failed', 'cancelled'].includes(readExecutionJob(results, id).status)) return;
  try { writeCampaignRecord(join(directoryFor(results, id), 'cancel.json'), { at: new Date().toISOString() }); }
  catch (error) { if (errorCode(error) !== 'EEXIST') throw error; }
}

/** One campaign per claim. External queues can run many workers on different hosts. */
export async function workExecutionJob(results: string, id: string, hostId: string,
  { env = process.env, signal, execute = executeCampaign }:
  { env?: NodeJS.ProcessEnv; signal?: AbortSignal; execute?: typeof executeCampaign } = {}) {
  name.parse(hostId);
  const current = readExecutionJob(results, id);
  if (current.status !== 'queued') return current;
  if (current.job.hostId && current.job.hostId !== hostId) throw new Error('job is assigned to another host');
  const directory = directoryFor(results, id);
  const planFile = join(directory, 'plan.json');
  const plan = compileCampaignFile(planFile);
  if (plan.contentSha256 !== current.job.planSha256) throw new Error('job plan identity changed');
  const claim = { hostId, token: sha256(randomUUID()), startedAt: new Date().toISOString() };
  try { writeCampaignRecord(join(directory, 'claim.json'), claim); }
  catch (error) { if (errorCode(error) !== 'EEXIST') throw error; return readExecutionJob(results, id); }
  // Claims never expire: a disconnected worker may still be making paid calls.
  const controller = new AbortController();
  const cancel = () => {
    if (signal?.aborted || existsSync(join(directory, 'cancel.json'))) controller.abort();
  };
  signal?.addEventListener('abort', cancel, { once: true });
  const timer = setInterval(cancel, 500);
  cancel();
  try {
    if (controller.signal.aborted) throw new Error('job cancelled before execution');
    const campaign = await execute(planFile, current.campaignDirectory, {
      mode: plan.state === 'draft' ? 'model-free-trial' : 'frozen',
      env, signal: controller.signal, executionCredentials: current.job.credentials,
      capacityPolicy: current.job.capacityPolicy,
      onCapacityWait: reason => writeCampaignRecord(join(directory, 'capacity.json'),
        { reason, at: new Date().toISOString() }, false),
    });
    const incomplete = campaign.summary.running > 0 || campaign.summary.pending > 0;
    const invalid = campaign.summary.invalid > 0;
    writeCampaignRecord(join(directory, 'result.json'), { hostId, token: claim.token,
      status: controller.signal.aborted ? 'cancelled' : incomplete || invalid ? 'failed' : 'completed',
      ...(incomplete ? { error: 'Campaign stopped with unfinished attempts. Inspect its evidence and reconcile owned resources before further work.' }
        : invalid ? { error: 'Campaign contains invalid executions. Inspect its evidence before further work.' } : {}),
      completedAt: new Date().toISOString(), campaign: campaign.summary });
  } catch (error) {
    writeCampaignRecord(join(directory, 'result.json'), { hostId, token: claim.token,
      status: controller.signal.aborted ? 'cancelled' : 'failed', completedAt: new Date().toISOString(),
      error: redactCredentials(error instanceof Error ? error.message : String(error)) });
  } finally { clearInterval(timer); signal?.removeEventListener('abort', cancel); }
  return readExecutionJob(results, id);
}
