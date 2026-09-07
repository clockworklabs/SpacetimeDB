import { randomUUID, createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { isIPv4 } from 'node:net';
import { chmodSync, closeSync, existsSync, fsyncSync, openSync, readFileSync, writeFileSync, renameSync,
  linkSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { validateStackLeaseResources } from '../stacks/stack-lease-capabilities.js';
import { resourceLockDescriptors } from './resource-lock-worker.js';
import type { ResourceLockTransaction } from './resource-lock-worker.js';
import { MAX_RUNNER_CAPACITY } from '../composition/product-config.js';

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
  workspaceHandedBackAt?: string;
  networkMode?: string;
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

export interface BackendLeaseNetwork {
  name: string;
  id: string;
  namespaceContainerId: string | null;
  namespaceStartedAt?: string;
  hostAddresses: string[];
  services: { address: string; port: number }[];
  /** This attempt's own addresses on its network, so a probe of its own application by
   *  bridge address is not read as another run. */
  ownAddresses?: string[];
  cacheContainerId?: string;
  firewallSha256: string | null;
  firewallInstalledAt: string | null;
}

export type BackendCreationKind = 'network' | 'backend' | 'build' | 'browser' | 'broker' | 'firewall' | 'smoke';
export type BackendCreationIntent = { name: string; creationToken: string };

export interface BackendLeaseResource {
  serverUri: string | null;
  dataDir: string | null;
  module: string | null;
  database: string | null;
  container: BackendLeaseContainer | null;
  buildContainer: BackendLeaseContainer | null;
  browserContainer?: BackendLeaseContainer;
  brokerContainer?: BackendLeaseContainer;
  smokeContainer?: BackendLeaseContainer;
  network?: BackendLeaseNetwork;
  creationIntents?: Partial<Record<BackendCreationKind, BackendCreationIntent>>;
  locks: BackendResourceLock[];
  lockIntent?: BackendResourceLock[];
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
  campaignDelegation?: { path: string; token: string };
  // Teardown stamps this when it stops a host it started.
  stoppedAt?: string;
  resources: BackendLeaseResource;
}

export type PublicBackendLease = Omit<BackendLease, 'ownershipToken' | 'campaignDelegation' | 'resources'> & {
  resources: Omit<BackendLeaseResource, 'creationIntents'>;
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

export const DEFAULT_SPACETIME_SERVER_URI = 'http://127.0.0.1:3210';

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
    validateStackLeaseResources(backend, {
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
    validateStackLeaseResources(leaseBackend, {
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
    if (buildContainer.workspaceHandedBackAt !== undefined
      && (typeof buildContainer.workspaceHandedBackAt !== 'string'
        || !Number.isFinite(Date.parse(buildContainer.workspaceHandedBackAt)))) {
      fail('buildContainer.workspaceHandedBackAt is invalid');
    }
    if (buildContainer.networkMode != null
      && (typeof buildContainer.networkMode !== 'string'
        || !/^(?:bridge|host|[a-f0-9]{64}|container:[a-f0-9]{64})$/.test(buildContainer.networkMode))) {
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
  if (resources.lockIntent !== undefined && !Array.isArray(resources.lockIntent)) {
    fail('lockIntent must be an array');
  }
  for (const key of ['browserContainer', 'brokerContainer', 'smokeContainer']) {
    const container = resources[key];
    if (container === undefined) continue;
    if (!isRecord(container) || container.owned !== true
      || typeof container.name !== 'string' || !/^[a-z0-9][a-z0-9_.-]{0,127}$/.test(container.name)
      || typeof container.id !== 'string' || !/^[a-f0-9]{64}$/.test(container.id)
      || typeof container.image !== 'string' || !/^sha256:[a-f0-9]{64}$/.test(container.image)
      || typeof container.networkMode !== 'string' || !/^container:[a-f0-9]{64}$/.test(container.networkMode)) {
      fail(`${key} must identify an owned container in an exact attempt namespace`);
    }
  }
  if (resources.creationIntents !== undefined) {
    if (!isRecord(resources.creationIntents)) fail('creationIntents must be an object');
    for (const [kind, intent] of Object.entries(resources.creationIntents)) {
      if (!['network', 'backend', 'build', 'browser', 'broker', 'firewall', 'smoke'].includes(kind)
        || !isRecord(intent) || Object.keys(intent).some(key => !['name', 'creationToken'].includes(key))
        || typeof intent.name !== 'string' || !/^[a-z0-9][a-z0-9_.-]{0,127}$/.test(intent.name)
        || typeof intent.creationToken !== 'string' || !/^[a-f0-9]{32,64}$/.test(intent.creationToken)) {
        fail('creation intent is invalid');
      }
    }
  }
  if (resources.network !== undefined) {
    const network = resources.network;
    if (!isRecord(network) || typeof network.name !== 'string' || !/^[a-z0-9][a-z0-9_.-]{0,127}$/.test(network.name)
      || typeof network.id !== 'string' || !/^[a-f0-9]{64}$/.test(network.id)
      || network.namespaceContainerId !== null && (typeof network.namespaceContainerId !== 'string'
        || !/^[a-f0-9]{64}$/.test(network.namespaceContainerId))
      || !Array.isArray(network.hostAddresses) || network.hostAddresses.some(value => typeof value !== 'string' || !isIPv4(value))
      || network.ownAddresses !== undefined && (!Array.isArray(network.ownAddresses)
        || network.ownAddresses.some(value => typeof value !== 'string' || !isIPv4(value)))
      || !Array.isArray(network.services) || network.services.some(value => !isRecord(value)
        || typeof value.address !== 'string' || !isIPv4(value.address)
        || typeof value.port !== 'number' || !Number.isInteger(value.port) || value.port < 1 || value.port > 65535)
      || network.firewallSha256 !== null && (typeof network.firewallSha256 !== 'string' || !/^[a-f0-9]{64}$/.test(network.firewallSha256))
      || network.firewallInstalledAt !== null && (typeof network.firewallInstalledAt !== 'string'
        || !Number.isFinite(Date.parse(network.firewallInstalledAt)))
      || network.cacheContainerId !== undefined && (typeof network.cacheContainerId !== 'string'
        || !/^[a-f0-9]{64}$/.test(network.cacheContainerId))) {
      fail('network must contain exact Docker identities and validated traffic endpoints');
    }
    if (network.namespaceContainerId !== null) {
      if (!isRecord(resources.container) || resources.container.id !== network.namespaceContainerId) {
        fail('network namespace must belong to the leased backend container');
      }
      for (const key of ['buildContainer', 'browserContainer', 'brokerContainer', 'smokeContainer']) {
        const container = resources[key];
        if (isRecord(container) && container.networkMode !== `container:${network.namespaceContainerId}`) {
          fail(`${key} is outside the leased network namespace`);
        }
      }
    }
    if ((network.firewallSha256 === null) !== (network.firewallInstalledAt === null)) {
      fail('firewall identity and installation time must be recorded together');
    }
    if (network.namespaceStartedAt !== undefined && (typeof network.namespaceStartedAt !== 'string'
      || !Number.isFinite(Date.parse(network.namespaceStartedAt)))) fail('namespace start identity is invalid');
  }
  for (const lock of [...resources.locks, ...(resources.lockIntent ?? [])]) {
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

export function writeBackendLease(path: string, lease: unknown,
  { exclusive = false }: { exclusive?: boolean } = {}): void {
  const validated = validateBackendLease(lease);
  const directory = dirname(path);
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  chmodSync(directory, 0o700);
  const temporary = `${path}.${process.pid}.${randomUUID()}.tmp`;
  const fd = openSync(temporary, 'wx', 0o600);
  try {
    writeFileSync(fd, `${JSON.stringify(validated, null, 2)}\n`);
    fsyncSync(fd);
  } finally { closeSync(fd); }
  if (exclusive) {
    try { linkSync(temporary, path); } finally { rmSync(temporary, { force: true }); }
  } else renameSync(temporary, path);
  if (process.platform === 'linux') {
    const directoryFd = openSync(directory, 'r');
    try { fsyncSync(directoryFd); } finally { closeSync(directoryFd); }
  }
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
  const { ownershipToken, campaignDelegation: _delegation, ...publicLease } = copy;
  delete publicLease.resources.creationIntents;
  return { ...publicLease, ownership: {
    markerSha256: createHash('sha256').update(ownershipToken).digest('hex'),
  } };
}


export function resourceLockScope(
  env: NodeJS.ProcessEnv = process.env,
  { temporaryDirectory = tmpdir() }: { temporaryDirectory?: string } = {},
): { root: string } {
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
  };
}

interface AppPorts { readonly vite: number; readonly express: number | null }

export function backendResourceLockKeys(
  lease: BackendLease,
  ports: AppPorts,
  additionalKeys: string[] = [],
  capacityIndex = 0,
): string[] {
  validateBackendLease(lease);
  return runResourceLockKeys({ ...lease, ports, serverUri: lease.resources.serverUri }, additionalKeys, capacityIndex);
}

export function runResourceLockKeys(
  run: { track: string; backend: string; runIndex: number; ports: AppPorts; serverUri?: string | null },
  additionalKeys: string[] = [], capacityIndex = 0,
): string[] {
  if (run.backend === 'stub') return [];
  if (!Array.isArray(additionalKeys)) fail('additional resource lock keys must be an array');
  for (const key of additionalKeys) requireString(key, 'resource lock key');
  if (!Number.isInteger(capacityIndex) || capacityIndex < 0 || capacityIndex >= MAX_RUNNER_CAPACITY) {
    fail('capacity index is outside the declared runner pool');
  }
  const ports = [run.ports.vite, run.ports.express, run.serverUri
    ? Number(loopbackHttpUri(run.serverUri).port) : null].filter((port): port is number => port !== null);
  if (ports.some(port => !Number.isInteger(port) || port < 1 || port > 65535)) {
    fail('resource lock ports must be assigned TCP ports');
  }
  return [...new Set([
    `capacity:runner:${capacityIndex}`,
    `slot:${run.track}:${run.backend}:run${run.runIndex}`,
    ...(run.serverUri ? [`listener:${run.serverUri}`] : []),
    ...ports.map(port => `port:${port}`),
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

function lockedResources(input: ResourceLockTransaction): BackendResourceLock[] {
  validateBackendLease(input.lease);
  requireString(input.root, 'lock root');
  for (const key of input.keys) requireString(key, 'lock key');
  if (!input.keys.length) return [];
  if (process.platform !== 'linux') {
    fail('resource-backed execution requires the Linux Docker appliance (flock)');
  }
  mkdirSync(input.root, { recursive: true, mode: 0o700 });
  // ponytail: one kernel mutex per lock root; split only if measured contention matters.
  // Never unlink this guard: every controller must lock the same inode.
  const result = spawnSync('flock', ['--exclusive', '--wait', '30',
    resolve(input.root, '.resource-lock.guard'), process.execPath,
    fileURLToPath(new URL('./resource-lock-worker.js', import.meta.url))], {
    input: JSON.stringify(input), encoding: 'utf8', timeout: 35_000,
    stdio: ['pipe', 'pipe', 'pipe'],
  });
  if (result.error || result.status !== 0) {
    fail(`resource ${input.operation} failed: ${result.error?.message
      ?? result.stderr.trim() ?? `flock exited ${result.status}`}`);
  }
  return JSON.parse(result.stdout) as BackendResourceLock[];
}

export function acquireResourceLock(input: {
  root: string; key: string; lease: BackendLease;
}): BackendResourceLock {
  return acquireResourceLocks({ ...input, keys: [input.key] })[0]!;
}

export function acquireResourceLocks(input: {
  root: string; keys: string[]; lease: BackendLease; capacity?: number;
}): BackendResourceLock[] {
  if (!Array.isArray(input.keys) || input.keys.length === 0) {
    fail('resource lock keys must be a non-empty array');
  }
  return lockedResources({ ...input, operation: 'acquire' });
}

/** Persist intended keys and private identity before any claim can survive a crash. */
export function claimBackendResources(path: string, lease: BackendLease, input: {
  root: string; keys: string[]; capacity?: number;
}): BackendLease {
  if (!input.keys.length) {
    writeBackendLease(path, lease);
    return lease;
  }
  // Record every candidate before the atomic worker selects one. Recovery only
  // releases intent records that match this lease's private ownership token.
  lease.resources.lockIntent = resourceLockDescriptors(input.root, input.keys, input.capacity);
  writeBackendLease(path, lease);
  lease.resources.locks = acquireResourceLocks({ ...input, lease });
  delete lease.resources.lockIntent;
  writeBackendLease(path, lease);
  return lease;
}

export function verifyResourceLocks(lease: BackendLease): void {
  for (const root of new Set(lease.resources.locks.map(lock => dirname(lock.path)))) {
    lockedResources({ root, lease, operation: 'verify',
      keys: lease.resources.locks.filter(lock => dirname(lock.path) === root).map(lock => lock.key) });
  }
}

export function releaseResourceLocks(lease: BackendLease): void {
  validateBackendLease(lease);
  for (const root of new Set(lease.resources.locks.map(lock => dirname(lock.path)))) {
    lockedResources({ root, lease, operation: 'release',
      keys: lease.resources.locks.filter(lock => dirname(lock.path) === root).map(lock => lock.key) });
  }
  for (const root of new Set(lease.resources.lockIntent?.map(lock => dirname(lock.path)))) {
    lockedResources({ root, lease, operation: 'release-intent',
      keys: lease.resources.lockIntent!.filter(lock => dirname(lock.path) === root).map(lock => lock.key) });
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
