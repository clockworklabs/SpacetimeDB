import { createHash, randomUUID } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { hostname } from 'node:os';
import { join, resolve } from 'node:path';

const VERSION = 2;
const HASH = /^[a-f0-9]{64}$/;
const SAFE_ID = /^[a-z0-9][a-z0-9.-]*$/;
const CONTAINER_HOSTNAME_MOUNT = /\/containers\/([a-f0-9]{64})\/hostname(?=\s|$)/;
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const tokenHash = (token: string): string => createHash('sha256').update(token).digest('hex');
const fail = (message: string): never => { throw new Error(`campaign lock: ${message}`); };

export interface CampaignIdentity {
  id: string;
  contentSha256: string;
}

export interface CampaignLockRecord {
  version: number;
  campaignId: string;
  campaignSha256: string;
  ownerPid: number;
  ownerInstance: string;
  ownershipMarkerSha256: string;
  acquiredAt: string;
}

export interface CampaignLock {
  path: string;
  token: string;
  record: CampaignLockRecord;
}

interface InspectResult {
  error?: Error;
  status: number | null;
  stdout?: string | null;
  stderr?: string | null;
}

type InspectController = (instance: string) => InspectResult;
type LockOwner = Pick<CampaignLockRecord, 'ownerInstance' | 'ownerPid'>;
type OwnerAlive = (record: LockOwner, currentInstance: string) => boolean;

function errorCode(error: unknown): string | undefined {
  return error instanceof Error && 'code' in error && typeof error.code === 'string'
    ? error.code : undefined;
}

function processAlive(pid: number): boolean {
  try { process.kill(pid, 0); return true; }
  catch (error) { return errorCode(error) === 'EPERM'; }
}

export function controllerInstance(env: NodeJS.ProcessEnv = process.env, {
  readMountInfo = () => readFileSync('/proc/self/mountinfo', 'utf8'),
  fallbackHostname = hostname,
}: { readMountInfo?: () => string; fallbackHostname?: () => string } = {}): string {
  const explicit = env.STACK_BENCH_CONTROLLER_INSTANCE?.trim();
  if (explicit) return explicit;
  try {
    const containerId = readMountInfo().match(CONTAINER_HOSTNAME_MOUNT)?.[1];
    if (containerId) return containerId;
  } catch { /* /proc is unavailable outside Linux containers */ }
  return fallbackHostname();
}

export function ownerAlive(record: LockOwner, currentInstance: string, {
  inspect = instance => spawnSync('docker', ['inspect', '--type', 'container', '--format',
    '{{.State.Running}}', instance], { encoding: 'utf8', stdio: 'pipe', timeout: 15_000 }),
}: { inspect?: InspectController } = {}): boolean {
  if (record.ownerInstance === currentInstance) return processAlive(record.ownerPid);
  if (!HASH.test(record.ownerInstance)) {
    fail(`cannot prove whether controller ${record.ownerInstance} is alive from ${currentInstance}`);
  }
  const result = inspect(record.ownerInstance);
  if (result.error) {
    fail(`cannot inspect controller ${record.ownerInstance}: ${result.error.message}`);
  }
  if (result.status === 0) {
    const running = String(result.stdout ?? '').trim();
    if (running === 'true') return true;
    if (running === 'false') return false;
    fail(`controller ${record.ownerInstance} returned an invalid Docker state`);
  }
  const detail = `${result.stderr ?? ''}${result.stdout ?? ''}`.trim();
  const escaped = record.ownerInstance.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  if (new RegExp(`(?:No such object|No such container):\\s*${escaped}(?:\\s|$)`, 'i').test(detail)) {
    return false;
  }
  return fail(`cannot determine whether controller ${record.ownerInstance} is alive: `
    + `${detail || `docker inspect exited ${result.status}`}`);
}

function validateRecord(input: unknown, path = '<campaign-lock>'): CampaignLockRecord {
  if (!object(input)) return fail(`${path} must contain an object`);
  const fields = new Set(['version', 'campaignId', 'campaignSha256', 'ownerPid',
    'ownerInstance', 'ownershipMarkerSha256', 'acquiredAt']);
  for (const key of Object.keys(input)) if (!fields.has(key)) fail(`${path}.${key} is unknown`);
  for (const key of fields) if (!Object.hasOwn(input, key)) fail(`${path}.${key} is required`);
  if (input.version !== VERSION) fail(`${path}.version is unsupported`);
  if (typeof input.campaignId !== 'string' || !SAFE_ID.test(input.campaignId)) {
    fail(`${path}.campaignId is invalid`);
  }
  for (const field of ['campaignSha256', 'ownershipMarkerSha256']) {
    if (typeof input[field] !== 'string' || !HASH.test(input[field])) fail(`${path}.${field} is invalid`);
  }
  if (typeof input.ownerPid !== 'number' || !Number.isInteger(input.ownerPid)
    || input.ownerPid <= 0) fail(`${path}.ownerPid is invalid`);
  if (typeof input.ownerInstance !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9_.-]{0,127}$/.test(input.ownerInstance)) {
    fail(`${path}.ownerInstance is invalid`);
  }
  if (typeof input.acquiredAt !== 'string' || Number.isNaN(Date.parse(input.acquiredAt))) {
    fail(`${path}.acquiredAt is invalid`);
  }
  return structuredClone(input) as unknown as CampaignLockRecord;
}

function readRecord(path: string): CampaignLockRecord | null {
  let parsed: unknown;
  try { parsed = JSON.parse(readFileSync(path, 'utf8')); }
  catch (error) {
    if (errorCode(error) === 'ENOENT') return null;
    fail(`${path} is unreadable; refusing to steal it`);
  }
  return validateRecord(parsed, path);
}

export function campaignLockIsActive(directory: string, campaign: CampaignIdentity,
  { currentInstance = controllerInstance(), alive = ownerAlive }:
  { currentInstance?: string; alive?: OwnerAlive } = {}): boolean {
  const path = join(resolve(directory), '.campaign.lock.json');
  const record = readRecord(path);
  if (record === null) return false;
  if (!campaign || record.campaignId !== campaign.id
    || record.campaignSha256 !== campaign.contentSha256) {
    fail(`${path} does not belong to the stored campaign`);
  }
  return alive(record, currentInstance);
}

export function acquireCampaignLock(directory: string, campaign: CampaignIdentity,
  { ownerPid = process.pid, ownerInstance = controllerInstance(), now = new Date().toISOString(), alive = ownerAlive,
    uuid = randomUUID }: { ownerPid?: number; ownerInstance?: string; now?: string;
      alive?: OwnerAlive; uuid?: () => string } = {}): CampaignLock {
  const root = resolve(directory);
  mkdirSync(root, { recursive: true });
  const path = join(root, '.campaign.lock.json');
  if (!campaign || typeof campaign.id !== 'string' || !SAFE_ID.test(campaign.id)
    || typeof campaign.contentSha256 !== 'string' || !HASH.test(campaign.contentSha256)) {
    fail('a compiled campaign identity is required');
  }
  if (!Number.isInteger(ownerPid) || ownerPid <= 0) fail('ownerPid is invalid');
  if (typeof ownerInstance !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9_.-]{0,127}$/.test(ownerInstance)) {
    fail('ownerInstance is invalid');
  }
  if (typeof now !== 'string' || Number.isNaN(Date.parse(now))) fail('acquiredAt is invalid');
  const token = uuid();
  const record = validateRecord({
    version: VERSION,
    campaignId: campaign.id,
    campaignSha256: campaign.contentSha256,
    ownerPid,
    ownerInstance,
    ownershipMarkerSha256: tokenHash(token),
    acquiredAt: now,
  });
  for (let attempt = 0; attempt < 4; attempt += 1) {
    try {
      writeFileSync(path, `${JSON.stringify(record, null, 2)}\n`, { flag: 'wx', mode: 0o600 });
      return { path, token, record };
    } catch (error) {
      if (errorCode(error) !== 'EEXIST') throw error;
      const existing = readRecord(path);
      if (existing === null) continue;
      if (alive(existing, ownerInstance)) {
        fail(`${campaign.id} is already controlled by ${existing.ownerInstance} pid ${existing.ownerPid}`);
      }
      const stale = `${path}.stale-${uuid()}`;
      try {
        renameSync(path, stale);
        rmSync(stale, { force: true });
      } catch (renameError) {
        if (errorCode(renameError) !== 'ENOENT') throw renameError;
      }
    }
  }
  return fail(`could not acquire ${campaign.id} after stale-lock contention`);
}

export function releaseCampaignLock(lock: CampaignLock): boolean {
  const input: unknown = lock;
  if (!object(input) || typeof input.path !== 'string' || typeof input.token !== 'string'
    || !object(input.record)) fail('release requires the acquired lock handle');
  const releasing = `${lock.path}.releasing-${randomUUID()}`;
  try { renameSync(lock.path, releasing); }
  catch (error) {
    if (errorCode(error) === 'ENOENT') return false;
    throw error;
  }
  const existing = readRecord(releasing);
  if (existing === null) return fail(`${releasing} disappeared while releasing the campaign lock`);
  const expected = validateRecord(lock.record);
  if (existing.campaignId !== expected.campaignId
    || existing.campaignSha256 !== expected.campaignSha256
    || existing.ownerPid !== expected.ownerPid
    || existing.ownerInstance !== expected.ownerInstance
    || existing.ownershipMarkerSha256 !== tokenHash(lock.token)) {
    try { renameSync(releasing, lock.path); }
    catch { /* preserve the moved evidence if another owner already acquired */ }
    fail(`${lock.path} no longer belongs to this controller; preserved ownership evidence at ${releasing}`);
  }
  rmSync(releasing);
  return true;
}
