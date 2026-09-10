import { constants, copyFileSync, existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { z } from 'zod';
import { campaignLockIsActive, readCampaignLock, writeCampaignRecord } from './campaign-lock.js';
import { campaignChildPath } from './campaign-path.js';
import { readCampaignState } from './campaign-scheduler.js';
import { ARTIFACT_FILE } from '../evidence/artifacts.js';
import { sha256 } from '../evidence/provenance.js';
import { hashAppSource } from '../runtime/source-snapshot.js';

const hash = z.string().regex(/^[a-f0-9]{64}$/);
const time = z.number().int().nonnegative().safe();
const identity = z.strictObject({ campaignSha256: hash, ownershipMarkerSha256: hash,
  depth: z.number().int().positive().safe() });
const contextSchema = identity.extend({ directory: z.string().min(1),
  attemptId: z.string().min(1), executionId: z.string().min(1) });
export type DepthPauseContext = z.infer<typeof contextSchema>;
const receiptSchema = contextSchema.omit({ directory: true }).extend({
  startedAt: time, resumedAt: time.nullable(), sourceSha256: hash, progressionSha256: hash,
}).refine(r => r.resumedAt === null || r.resumedAt >= r.startedAt,
  'pause ends before it starts');
const releaseSchema = identity.extend({ releasedAt: time });
const RECEIPT = 'depth-pause.json';
const RELEASE = 'depth-release.json';

export function readDepthPauseContext(env = process.env): DepthPauseContext | null {
  return env.STACK_BENCH_DEPTH_PAUSE_CONTEXT
    ? contextSchema.parse(JSON.parse(env.STACK_BENCH_DEPTH_PAUSE_CONTEXT)) : null;
}

export function readDepthPause(output: string, context: DepthPauseContext) {
  const path = join(output, RECEIPT);
  if (!existsSync(path)) return null;
  const receipt = receiptSchema.parse(JSON.parse(readFileSync(path, 'utf8')));
  for (const key of ['campaignSha256', 'ownershipMarkerSha256', 'depth', 'attemptId', 'executionId'] as const) {
    if (receipt[key] !== context[key]) throw new Error(`depth pause ${key} changed`);
  }
  return receipt;
}

/** Trusted controller reads this receipt; coding agents never receive its path. */
export function depthPauseDurationMs(output: string, context: DepthPauseContext, now = Date.now()): number {
  const receipt = readDepthPause(output, context);
  if (!receipt) return 0;
  if (receipt.startedAt > now || (receipt.resumedAt !== null && receipt.resumedAt > now)) {
    throw new Error('depth pause contains a future timestamp');
  }
  return (receipt.resumedAt ?? now) - receipt.startedAt;
}

/** Keep the same process, source, database, leases, and progression object. */
export async function waitAtDepthBoundary(output: string, app: string, context: DepthPauseContext,
  { signal }: { signal?: AbortSignal } = {}): Promise<number> {
  contextSchema.parse(context);
  if (existsSync(join(output, RECEIPT))) throw new Error('depth pause was already entered');
  const sourceSha256 = hashAppSource(app).sha256;
  const statePath = join(output, ARTIFACT_FILE.progressionState);
  const progressionSha256 = sha256(readFileSync(statePath));
  copyFileSync(statePath, join(output, 'depth-pause-state.json'), constants.COPYFILE_EXCL);
  const { directory, ...binding } = context;
  const receipt = { ...binding, startedAt: Date.now(), resumedAt: null as number | null,
    sourceSha256, progressionSha256 };
  writeCampaignRecord(join(output, RECEIPT), receipt);
  const releasePath = campaignChildPath(directory, RELEASE, 'depth release');
  console.log(`Paused after depth ${context.depth}. Waiting for campaign continue-depth.`);
  while (true) {
    signal?.throwIfAborted();
    if (existsSync(releasePath)) {
      const release = releaseSchema.parse(JSON.parse(readFileSync(releasePath, 'utf8')));
      for (const key of ['campaignSha256', 'ownershipMarkerSha256', 'depth'] as const) {
        if (release[key] !== context[key]) throw new Error(`depth release ${key} changed`);
      }
      if (release.releasedAt > Date.now()) throw new Error('depth release is in the future');
      if (hashAppSource(app).sha256 !== sourceSha256
        || sha256(readFileSync(statePath)) !== progressionSha256) {
        throw new Error('source or progression changed during the depth pause');
      }
      receipt.resumedAt = Date.now();
      writeCampaignRecord(join(output, RECEIPT), receipt, false);
      return receipt.resumedAt - receipt.startedAt;
    }
    await delay(250, undefined, { signal });
  }
}

export function validateDepthPauseEvidence(output: string, expected: {
  campaignSha256: string; attemptId: string; depth: number; durationMs: number;
}): void {
  const receipt = receiptSchema.parse(JSON.parse(readFileSync(join(output, RECEIPT), 'utf8')));
  if (receipt.campaignSha256 !== expected.campaignSha256 || receipt.attemptId !== expected.attemptId
    || receipt.depth !== expected.depth || receipt.resumedAt === null
    || receipt.resumedAt - receipt.startedAt !== expected.durationMs
    || receipt.progressionSha256 !== sha256(readFileSync(join(output, 'depth-pause-state.json')))) {
    throw new Error('run pause accounting does not match its boundary evidence');
  }
}

export function campaignDepthPauseStatus(directory: string) {
  const { plan, state } = readCampaignState(directory, { requireCurrentInputs: false });
  const lock = readCampaignLock(directory);
  const depth = plan.definition.mode.pauseAfterDepth;
  if (depth === undefined || !lock || !campaignLockIsActive(directory, plan)) {
    throw new Error('depth pause requires a planned boundary and a live controller');
  }
  const binding = { campaignSha256: plan.contentSha256,
    ownershipMarkerSha256: lock.ownershipMarkerSha256, depth };
  const attempts = state.attempts.map(attempt => {
    const execution = attempt.executions.at(-1);
    const receipt = execution && readDepthPause(campaignChildPath(directory, execution.output,
      'depth pause execution'), { ...binding, directory, attemptId: attempt.plan.id,
      executionId: execution.id });
    return { attemptId: attempt.plan.id, status: attempt.status,
      paused: attempt.status === 'running' && receipt != null && receipt.resumedAt === null,
      receipt: receipt ?? null };
  });
  return { ...binding, attempts };
}

/** Release the cohort together. Failed attempts remain in the original cohort. */
export function continueCampaignDepth(directory: string) {
  const status = campaignDepthPauseStatus(directory);
  const path = campaignChildPath(directory, RELEASE, 'depth release');
  if (existsSync(path)) {
    const prior = releaseSchema.parse(JSON.parse(readFileSync(path, 'utf8')));
    if (prior.campaignSha256 !== status.campaignSha256
      || prior.ownershipMarkerSha256 !== status.ownershipMarkerSha256 || prior.depth !== status.depth) {
      throw new Error('depth release belongs to a different controller or campaign');
    }
    return prior;
  }
  if (!status.attempts.some(a => a.paused)
    || status.attempts.some(a => a.status === 'pending' || (a.status === 'running' && !a.paused))) {
    throw new Error('wait until every attempt is at the depth boundary or terminal');
  }
  const release = { campaignSha256: status.campaignSha256,
    ownershipMarkerSha256: status.ownershipMarkerSha256, depth: status.depth, releasedAt: Date.now() };
  writeCampaignRecord(path, release);
  return release;
}
