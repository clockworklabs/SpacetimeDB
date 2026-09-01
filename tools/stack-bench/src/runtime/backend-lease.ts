import { randomUUID, createHash } from 'node:crypto';
import { chmodSync, constants, copyFileSync, existsSync, readFileSync, writeFileSync, renameSync,
  mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { executeStackLeaseCapability } from '../stacks/stack-lease-capabilities.js';
import { processIdentity } from './platform.js';

export const LEASE_VERSION = 1;
const LEASE_STATES = new Set<string>(['created', 'starting', 'active', 'restarting',
  'retained', 'stopped', 'released']);

export type BackendLeaseState = 'created' | 'starting' | 'active' | 'restarting'
  | 'retained' | 'stopped' | 'released';

export interface BackendLeaseContainer {
  name: string;
  id: string;
  image?: string;
  owned?: boolean;
  running?: boolean;
  removedAt?: string;
  networkMode?: 'bridge' | 'host';
  resourceLimits?: {
    cpuCount: number;
    memoryBytes: number;
    memorySwapBytes: number;
    pids: number;
  };
}

export interface BackendResourceLock {
  path: string;
  key: string;
  digest: string;
  acquiredAt?: string;
  releasedAt?: string;
}

export interface BackendProcessIdentity { pid: number; startMarker: string }

export interface BackendLeaseResource {
  serverUri: string | null;
  dataDir: string | null;
  module: string | null;
  database: string | null;
  container: BackendLeaseContainer | null;
  buildContainer: BackendLeaseContainer | null;
  locks: BackendResourceLock[];
  launchedProcess: BackendProcessIdentity | null;
  listenerProcesses: BackendProcessIdentity[];
}

export interface BackendLease {
  version: typeof LEASE_VERSION;
  backend: string;
  runId: string;
  track: string;
  runIndex: number;
  ownerPid: number;
  ownershipToken: string;
  createdAt: string;
  state: BackendLeaseState;
  releasedAt?: string;
  // Teardown stamps this when it stops a host it started.
  stoppedAt?: string;
  resources: BackendLeaseResource;
}

export type PublicBackendLease = Omit<BackendLease, 'ownershipToken'> & {
  ownership: { markerSha256: string };
};

export interface SpacetimeBackendLease extends BackendLease {
  backend: 'spacetime';
  resources: BackendLeaseResource & { serverUri: string; module: string };
}

export interface BackendLeaseExpectation {
  token?: string;
  backend?: string;
  runId?: string;
  active?: boolean;
}

interface CreateBackendLeaseInput {
  runId: string;
  backend: string;
  track: string;
  runIndex: number;
  ownerPid?: number;
  serverUri?: string | null;
  database?: string | null;
  module?: string | null;
  dataDir?: string | null;
  container?: Pick<BackendLeaseContainer, 'name' | 'id'> | null;
}

function fail(message: string): never {
  throw new Error(`invalid backend lease: ${message}`);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function errorCode(error: unknown): unknown {
  return isRecord(error) ? error.code : undefined;
}

function requireString(value: unknown, name: string): string {
  if (typeof value !== 'string' || !value.trim()) fail(`${name} is required`);
  return value;
}

export function loopbackHttpUri(value: unknown): URL {
  let url: URL;
  try { url = new URL(String(value)); } catch { fail(`serverUri is not a URL: ${value}`); }
  if (url.protocol !== 'http:') fail(`serverUri must use http: ${value}`);
  if (!['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname) || !url.port) {
    fail(`serverUri must name an explicit loopback port: ${value}`);
  }
  return url;
}

export function newRunId({ track, backend, runIndex, now = new Date(), nonce = randomUUID() }: {
  track: string;
  backend: string;
  runIndex: number;
  now?: Date;
  nonce?: string;
}): string {
  const stamp = now.toISOString().replace(/[-:.TZ]/g, '').slice(0, 14);
  const safe = (value: unknown): string => String(value).toLowerCase().replace(/[^a-z0-9_-]+/g, '-');
  return `${safe(track)}-${safe(backend)}-run${Number(runIndex)}-${stamp}-${safe(nonce).slice(0, 8)}`;
}

export function createBackendLease({ runId, backend, track, runIndex, ownerPid = process.pid,
  serverUri = null, database = null, module = null, dataDir = null,
  container = null }: CreateBackendLeaseInput): BackendLease {
  requireString(runId, 'runId');
  requireString(backend, 'backend');
  requireString(track, 'track');
  if (!Number.isInteger(runIndex) || runIndex < 0) fail('runIndex must be a non-negative integer');
  if (!Number.isInteger(ownerPid) || ownerPid <= 0) fail('ownerPid must be a positive integer');
  const resources = {
    serverUri,
    database,
    module,
    dataDir: dataDir ? resolve(dataDir) : null,
    container: container ? { name: container.name, id: container.id, owned: false } : null,
    buildContainer: null,
    locks: [],
    launchedProcess: null,
    listenerProcesses: [],
  };
  try {
    executeStackLeaseCapability(backend, 'validate-resources', {
      resources,
      helpers: { requireString, loopbackHttpUri },
    });
  } catch (error) {
    const message = errorMessage(error);
    if (message.startsWith('invalid backend lease:')) throw error;
    fail(message);
  }
  return {
    version: LEASE_VERSION,
    runId,
    backend,
    track,
    runIndex,
    ownerPid,
    ownershipToken: randomUUID(),
    createdAt: new Date().toISOString(),
    state: 'created',
    resources,
  };
}

function hasBackendLeaseShape(
  lease: Record<string, unknown>,
): lease is Record<string, unknown> & BackendLease {
  return typeof lease.runId === 'string'
    && typeof lease.backend === 'string'
    && typeof lease.track === 'string'
    && typeof lease.runIndex === 'number'
    && typeof lease.ownerPid === 'number'
    && typeof lease.ownershipToken === 'string'
    && typeof lease.createdAt === 'string'
    && typeof lease.state === 'string' && LEASE_STATES.has(lease.state)
    && isRecord(lease.resources);
}

export function validateBackendLease(
  lease: unknown,
  { token, backend, runId, active = false }: BackendLeaseExpectation = {},
): BackendLease {
  if (!isRecord(lease)) fail('document is not an object');
  if (lease.version !== LEASE_VERSION) fail(`unsupported version ${lease.version}`);
  const leaseRunId = requireString(lease.runId, 'runId');
  const leaseBackend = requireString(lease.backend, 'backend');
  const leaseToken = requireString(lease.ownershipToken, 'ownershipToken');
  if (typeof lease.state !== 'string' || !LEASE_STATES.has(lease.state)) {
    fail(`unknown state ${lease.state}`);
  }
  if (token !== undefined && token !== leaseToken) fail('ownership token does not match');
  if (backend !== undefined && backend !== leaseBackend) fail(`backend is ${leaseBackend}, not ${backend}`);
  if (runId !== undefined && runId !== leaseRunId) fail(`runId is ${leaseRunId}, not ${runId}`);
  if (active && !['active', 'restarting'].includes(lease.state)) fail(`lease is ${lease.state}, not active`);
  try {
    executeStackLeaseCapability(leaseBackend, 'validate-resources', {
      resources: lease.resources,
      helpers: { requireString, loopbackHttpUri },
    });
  } catch (error) {
    const message = errorMessage(error);
    if (message.startsWith('invalid backend lease:')) throw error;
    fail(message);
  }
  if (!isRecord(lease.resources)) fail('resources must be an object');
  const resources = lease.resources;
  if (resources.buildContainer != null) {
    if (!isRecord(resources.buildContainer)) fail('buildContainer must be an object');
    const buildContainer = resources.buildContainer;
    requireString(buildContainer.name, 'buildContainer.name');
    requireString(buildContainer.id, 'buildContainer.id');
    requireString(buildContainer.image, 'buildContainer.image');
    if (buildContainer.owned !== true) fail('buildContainer must be benchmark-owned');
    if (buildContainer.networkMode != null
      && (typeof buildContainer.networkMode !== 'string'
        || !['bridge', 'host'].includes(buildContainer.networkMode))) {
      fail('buildContainer.networkMode is invalid');
    }
    const limits = buildContainer.resourceLimits;
    const fields = ['cpuCount', 'memoryBytes', 'memorySwapBytes', 'pids'];
    if (!isRecord(limits)
      || Object.keys(limits).some(key => !fields.includes(key))
      || fields.some(field => !Number.isSafeInteger(limits[field])
        || typeof limits[field] !== 'number' || limits[field] < 1)
      || typeof limits.memorySwapBytes !== 'number'
      || typeof limits.memoryBytes !== 'number'
      || limits.memorySwapBytes < limits.memoryBytes) {
      fail('buildContainer.resourceLimits is invalid');
    }
  }
  if (!Array.isArray(resources.locks)) fail('locks must be an array');
  const validProcess = (value: unknown): boolean => isRecord(value)
    && Object.keys(value).every(key => ['pid', 'startMarker'].includes(key))
    && Number.isSafeInteger(value.pid) && typeof value.pid === 'number' && value.pid > 0
    && typeof value.startMarker === 'string' && /^\d+$/.test(value.startMarker);
  if (resources.launchedProcess !== null && !validProcess(resources.launchedProcess)) {
    fail('launchedProcess must be a process identity or null');
  }
  if (!Array.isArray(resources.listenerProcesses)
    || resources.listenerProcesses.some((identity: unknown) => !validProcess(identity))) {
    fail('listenerProcesses must contain only process identities');
  }
  for (const lock of resources.locks) {
    if (!isRecord(lock)) fail('lock must be an object');
    requireString(lock.path, 'lock.path');
    requireString(lock.key, 'lock.key');
    requireString(lock.digest, 'lock.digest');
  }
  if (!hasBackendLeaseShape(lease)) fail('document shape is incomplete');
  return lease;
}

export function readBackendLease(path: string, expected: BackendLeaseExpectation = {}): BackendLease {
  requireString(path, 'path');
  let parsed: unknown;
  try { parsed = JSON.parse(readFileSync(path, 'utf8')); }
  catch (error) { fail(`cannot read ${path}: ${errorMessage(error)}`); }
  return validateBackendLease(parsed, expected);
}

export function writeBackendLease(path: string, lease: unknown): void {
  const validated = validateBackendLease(lease);
  const directory = dirname(path);
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  chmodSync(directory, 0o700);
  const temporary = `${path}.${process.pid}.${randomUUID()}.tmp`;
  writeFileSync(temporary, `${JSON.stringify(validated, null, 2)}\n`, { flag: 'wx', mode: 0o600 });
  renameSync(temporary, path);
  chmodSync(path, 0o600);
}

export function updateBackendLease(
  path: string,
  expected: BackendLeaseExpectation,
  update: (lease: BackendLease) => BackendLease | void,
): BackendLease {
  const lease = readBackendLease(path, expected);
  const next = update(structuredClone(lease)) ?? lease;
  validateBackendLease(next, { token: lease.ownershipToken, backend: lease.backend, runId: lease.runId });
  writeBackendLease(path, next);
  return next;
}

export function publicBackendLease(lease: BackendLease): PublicBackendLease {
  const copy = structuredClone(validateBackendLease(lease));
  const { ownershipToken, ...publicLease } = copy;
  return { ...publicLease, ownership: {
    markerSha256: createHash('sha256').update(ownershipToken).digest('hex'),
  } };
}

const tokenHash = (token: string): string => createHash('sha256').update(token).digest('hex');

export function resourceLockScope(
  env: NodeJS.ProcessEnv = process.env,
  { temporaryDirectory = tmpdir() }: { temporaryDirectory?: string } = {},
): { root: string; reclaimStale: boolean } {
  const configured = env.STACK_BENCH_RESOURCE_LOCK_DIR;
  if (configured !== undefined) {
    if (typeof configured !== 'string' || configured !== configured.trim()
      || !configured || !isAbsolute(configured)) {
      fail('STACK_BENCH_RESOURCE_LOCK_DIR must be an absolute path without surrounding whitespace');
    }
  }
  const appliance = env.STACK_BENCH_APPLIANCE === '1';
  return {
    root: configured ?? (appliance
      ? '/var/lib/stack-bench/controller-home/resource-locks'
      : join(temporaryDirectory, 'stack-bench-resource-locks')),
    // Controller containers do not share a PID namespace. A PID that appears
    // absent in one container can still own a lock in another container. The
    // appliance therefore leaves stale-lock release to authenticated recovery.
    reclaimStale: !appliance,
  };
}

export function backendResourceLockKeys(
  lease: BackendLease,
  additionalKeys: string[] = [],
): string[] {
  validateBackendLease(lease);
  if (!Array.isArray(additionalKeys)) fail('additional resource lock keys must be an array');
  for (const key of additionalKeys) requireString(key, 'resource lock key');
  return [...new Set([
    `slot:${lease.track}:${lease.backend}:run${lease.runIndex}`,
    ...additionalKeys,
  ])].sort();
}

export function existingResourceLockKeys(
  { root, keys }: { root: string; keys: string[] },
): string[] {
  requireString(root, 'lock root');
  if (!Array.isArray(keys) || keys.length === 0) fail('resource lock keys must be a non-empty array');
  for (const key of keys) requireString(key, 'resource lock key');
  return [...new Set(keys)].sort().filter(key => {
    const digest = createHash('sha256').update(key).digest('hex');
    return existsSync(resolve(root, `${digest}.lock.json`));
  });
}

function processAlive(pid: number, startMarker: unknown): boolean {
  if (typeof startMarker === 'string' && startMarker) {
    return processIdentity(pid)?.startMarker === startMarker;
  }
  try { process.kill(pid, 0); return true; }
  catch (error) { return errorCode(error) === 'EPERM'; }
}

export function acquireResourceLock({ root, key, lease, reclaimStale = true }: {
  root: string;
  key: string;
  lease: BackendLease;
  reclaimStale?: boolean;
}): BackendResourceLock {
  validateBackendLease(lease);
  requireString(root, 'lock root');
  requireString(key, 'lock key');
  mkdirSync(root, { recursive: true });
  const digest = createHash('sha256').update(key).digest('hex');
  const path = resolve(root, `${digest}.lock.json`);
  const record = {
    version: 1,
    key,
    runId: lease.runId,
    ownerPid: lease.ownerPid,
    ownerStartMarker: processIdentity(lease.ownerPid)?.startMarker ?? null,
    ownershipMarkerSha256: tokenHash(lease.ownershipToken),
    acquiredAt: new Date().toISOString(),
  };
  for (let attempt = 0; attempt < 4; attempt++) {
    try {
      writeFileSync(path, `${JSON.stringify(record, null, 2)}\n`, { flag: 'wx' });
      return { path, key, digest, acquiredAt: record.acquiredAt };
    } catch (error) {
      if (errorCode(error) !== 'EEXIST') throw error;
      let existing: unknown;
      try { existing = JSON.parse(readFileSync(path, 'utf8')); }
      catch { fail(`resource lock ${path} exists but is unreadable; refusing to steal it`); }
      if (!isRecord(existing) || !Number.isInteger(existing.ownerPid)
        || typeof existing.ownerPid !== 'number'
        || typeof existing.runId !== 'string' || !existing.runId
        || (existing.ownerStartMarker !== null && typeof existing.ownerStartMarker !== 'string')
        || typeof existing.ownershipMarkerSha256 !== 'string'
        || !existing.ownershipMarkerSha256) {
        fail(`resource lock ${path} is malformed; refusing to steal it`);
      }
      if (processAlive(existing.ownerPid, existing.ownerStartMarker)) {
        fail(`resource ${key} is already leased by ${existing.runId} (pid ${existing.ownerPid})`);
      }
      if (!reclaimStale) {
        fail(`resource ${key} remains leased by ${existing.runId}; run authenticated recovery before reuse`);
      }
      // Move the exact stale inode out of the acquisition path. If another
      // contender won that race, rename fails and the next iteration rereads
      // the new owner's record rather than deleting it.
      const stale = `${path}.stale-${randomUUID()}`;
      try {
        renameSync(path, stale);
        rmSync(stale, { force: true });
      } catch (renameError) {
        if (errorCode(renameError) !== 'ENOENT') throw renameError;
      }
    }
  }
  fail(`could not acquire resource ${key} after stale-lock contention`);
}

export function acquireResourceLocks({ root, keys, lease, reclaimStale = true }: {
  root: string;
  keys: string[];
  lease: BackendLease;
  reclaimStale?: boolean;
}): BackendResourceLock[] {
  validateBackendLease(lease);
  if (!Array.isArray(keys) || keys.length === 0) fail('resource lock keys must be a non-empty array');
  const acquired: BackendResourceLock[] = [];
  try {
    for (const key of [...new Set(keys)].sort()) {
      acquired.push(acquireResourceLock({ root, key, lease, reclaimStale }));
    }
    return acquired;
  } catch (error) {
    const rollbackLease = structuredClone(lease);
    rollbackLease.resources.locks = acquired;
    try { releaseResourceLocks(rollbackLease); }
    catch (rollbackError) {
      throw new AggregateError([error, rollbackError],
        `resource lock acquisition failed and rollback was incomplete: ${errorMessage(error)}`);
    }
    throw error;
  }
}

export function releaseResourceLocks(lease: BackendLease): void {
  validateBackendLease(lease);
  for (const lock of lease.resources.locks ?? []) {
    const releasing = `${lock.path}.release-${randomUUID()}`;
    try { renameSync(lock.path, releasing); }
    catch (error) {
      if (errorCode(error) === 'ENOENT') continue;
      fail(`cannot claim resource lock ${lock.path} for release: ${errorMessage(error)}`);
    }
    let existing: unknown;
    try { existing = JSON.parse(readFileSync(releasing, 'utf8')); }
    catch (error) {
      try {
        copyFileSync(releasing, lock.path, constants.COPYFILE_EXCL);
        rmSync(releasing, { force: true });
      } catch { /* preserve both records for authenticated recovery */ }
      fail(`cannot read claimed resource lock ${releasing}: ${errorMessage(error)}`);
    }
    if (!isRecord(existing)
      || existing.runId !== lease.runId
      || existing.ownerPid !== lease.ownerPid
      || existing.ownershipMarkerSha256 !== tokenHash(lease.ownershipToken)) {
      try {
        copyFileSync(releasing, lock.path, constants.COPYFILE_EXCL);
        rmSync(releasing, { force: true });
      } catch { /* preserve both records for authenticated recovery */ }
      fail(`resource lock ${lock.path} no longer belongs to lease ${lease.runId}`);
    }
    rmSync(releasing, { force: true });
  }
}

export function leaseFromEnv(
  env: NodeJS.ProcessEnv | undefined,
  expected: BackendLeaseExpectation & { backend: 'spacetime' },
): { path: string; lease: SpacetimeBackendLease };
export function leaseFromEnv(
  env?: NodeJS.ProcessEnv,
  expected?: BackendLeaseExpectation,
): { path: string; lease: BackendLease };
export function leaseFromEnv(
  env: NodeJS.ProcessEnv = process.env,
  expected: BackendLeaseExpectation = {},
): { path: string; lease: BackendLease } {
  const path = requireString(env.STACK_BENCH_LEASE, 'STACK_BENCH_LEASE');
  const token = requireString(env.STACK_BENCH_LEASE_TOKEN, 'STACK_BENCH_LEASE_TOKEN');
  return { path, lease: readBackendLease(path, { ...expected, token }) };
}
