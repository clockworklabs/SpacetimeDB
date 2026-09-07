import { createHash, randomUUID } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { closeSync, fsyncSync, linkSync, mkdirSync, openSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { hostname } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

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

export type CampaignLockTransaction =
  | { operation: 'acquire' | 'release'; lock: CampaignLock }
  | { operation: 'cancel'; path: string; campaign: CampaignIdentity;
      ownershipMarkerSha256: string; requestedAt: string };

function syncDirectory(root: string): void {
  if (process.platform !== 'linux') return;
  const fd = openSync(root, 'r');
  try { fsyncSync(fd); } finally { closeSync(fd); }
}

function publish(path: string, record: unknown): void {
  const temporary = `${path}.${randomUUID()}.tmp`;
  const fd = openSync(temporary, 'wx', 0o600);
  try {
    writeFileSync(fd, `${JSON.stringify(record)}\n`);
    fsyncSync(fd);
  } finally { closeSync(fd); }
  try { linkSync(temporary, path); }
  finally { rmSync(temporary, { force: true }); }
  syncDirectory(dirname(path));
}

// Pure transaction for portable record tests. Production always calls under flock.
export function campaignLockTransaction(input: CampaignLockTransaction,
  { alive = ownerAlive }: { alive?: OwnerAlive } = {}): boolean {
  const path = input.operation === 'cancel' ? input.path : input.lock.path;
  const existing = readRecord(path);
  if (input.operation === 'cancel') {
    if (!HASH.test(input.ownershipMarkerSha256)) fail('cancellation owner marker is invalid');
    if (!existing || existing.ownershipMarkerSha256 !== input.ownershipMarkerSha256) return false;
    if (existing.campaignId !== input.campaign.id
      || existing.campaignSha256 !== input.campaign.contentSha256) fail('cancellation campaign does not match owner');
    const cancellation = `${path}.cancel`;
    // Repeated Stop requests for one owner are idempotent.
    try { publish(cancellation, { ...input, version: 1 }); }
    catch (error) { if (errorCode(error) !== 'EEXIST') throw error; }
    return true;
  }
  const { lock } = input;
  const expected = validateRecord(lock.record);
  if (expected.ownershipMarkerSha256 !== tokenHash(lock.token)) fail('lock handle token does not match owner');
  if (input.operation === 'release') {
    if (!existing) return false;
    if (JSON.stringify(existing) !== JSON.stringify(expected)) {
      fail(`${path} no longer belongs to this controller`);
    }
    rmSync(path);
    rmSync(`${path}.cancel`, { force: true });
    syncDirectory(dirname(path));
    return true;
  }
  if (existing) {
    if (existing.campaignId !== expected.campaignId
      || existing.campaignSha256 !== expected.campaignSha256) fail('existing lock belongs to a different campaign');
    if (alive(existing, expected.ownerInstance)) {
      fail(`${expected.campaignId} is already controlled by ${existing.ownerInstance} pid ${existing.ownerPid}`);
    }
    rmSync(path);
  }
  rmSync(`${path}.cancel`, { force: true });
  publish(path, expected);
  return true;
}

function lockedTransaction(input: CampaignLockTransaction): boolean {
  if (process.platform !== 'linux') fail('campaign control requires the Linux Docker appliance (flock)');
  const path = input.operation === 'cancel' ? input.path : input.lock.path;
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  // Never unlink this guard: all campaign state writers must lock the same inode.
  const result = spawnSync('flock', ['--exclusive', '--wait', '30',
    join(dirname(path), '.campaign.guard'), process.execPath,
    fileURLToPath(import.meta.url), '--transaction'], {
    input: JSON.stringify(input), encoding: 'utf8', timeout: 50_000,
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  if (result.error || result.status !== 0) {
    fail(result.error?.message || result.stderr.trim() || `flock exited ${result.status}`);
  }
  return JSON.parse(result.stdout) as boolean;
}

export function acquireCampaignLock(directory: string, campaign: CampaignIdentity,
  { ownerPid = process.pid, ownerInstance = controllerInstance(), now = new Date().toISOString(),
    uuid = randomUUID }: { ownerPid?: number; ownerInstance?: string; now?: string;
      uuid?: () => string } = {}): CampaignLock {
  const token = uuid();
  const lock = { path: join(resolve(directory), '.campaign.lock.json'), token,
    record: validateRecord({ version: VERSION, campaignId: campaign.id,
      campaignSha256: campaign.contentSha256, ownerPid, ownerInstance,
      ownershipMarkerSha256: tokenHash(token), acquiredAt: now }) };
  lockedTransaction({ operation: 'acquire', lock });
  return lock;
}

export function releaseCampaignLock(lock: CampaignLock): boolean {
  return lockedTransaction({ operation: 'release', lock });
}

export function readCampaignLock(directory: string): CampaignLockRecord | null {
  return readRecord(join(resolve(directory), '.campaign.lock.json'));
}

export function requestCampaignCancellation(directory: string, campaign: CampaignIdentity,
  expectedOwnershipMarkerSha256: string): boolean {
  return lockedTransaction({ operation: 'cancel', path: join(resolve(directory), '.campaign.lock.json'),
    campaign, ownershipMarkerSha256: expectedOwnershipMarkerSha256,
    requestedAt: new Date().toISOString() });
}

export function campaignCancellationRequested(lock: CampaignLock): boolean {
  let request: unknown;
  try { request = JSON.parse(readFileSync(`${lock.path}.cancel`, 'utf8')); }
  catch (error) { if (errorCode(error) === 'ENOENT') return false; throw error; }
  if (!object(request) || request.version !== 1 || !object(request.campaign)) {
    return fail('cancellation request is malformed');
  }
  return request.ownershipMarkerSha256 === lock.record.ownershipMarkerSha256
    && request.campaign.id === lock.record.campaignId
    && request.campaign.contentSha256 === lock.record.campaignSha256;
}

export function watchCampaignCancellation(lock: CampaignLock, externalSignal?: AbortSignal | null):
  { signal: AbortSignal; poll: () => void; close: () => void } {
  const controller = new AbortController();
  const abort = (): void => controller.abort(externalSignal?.reason);
  if (externalSignal?.aborted) abort();
  else externalSignal?.addEventListener('abort', abort, { once: true });
  const check = (): void => {
    try { if (campaignCancellationRequested(lock)) controller.abort(new Error('campaign cancellation requested')); }
    catch (error) { controller.abort(error); }
  };
  check();
  const timer = setInterval(check, 250);
  timer.unref();
  return { signal: controller.signal, poll: check, close: () => {
    clearInterval(timer);
    externalSignal?.removeEventListener('abort', abort);
  } };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href
  && process.argv[2] === '--transaction') {
  try { process.stdout.write(JSON.stringify(campaignLockTransaction(
    JSON.parse(readFileSync(0, 'utf8')) as CampaignLockTransaction))); }
  catch (error) {
    process.stderr.write(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
