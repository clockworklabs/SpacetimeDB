import { randomUUID, createHash } from 'node:crypto';
import { readFileSync, writeFileSync, renameSync, mkdirSync, rmSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { executeStackLeaseCapability } from '../stacks/stack-lease-capabilities.mjs';

export const LEASE_VERSION = 1;
const LEASE_STATES = new Set(['created', 'starting', 'active', 'restarting',
  'retained', 'stopped', 'released']);

const fail = message => { throw new Error(`invalid backend lease: ${message}`); };

function requireString(value, name) {
  if (typeof value !== 'string' || !value.trim()) fail(`${name} is required`);
  return value;
}

export function loopbackHttpUri(value) {
  let url;
  try { url = new URL(value); } catch { fail(`serverUri is not a URL: ${value}`); }
  if (url.protocol !== 'http:') fail(`serverUri must use http: ${value}`);
  if (!['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname) || !url.port) {
    fail(`serverUri must name an explicit loopback port: ${value}`);
  }
  return url;
}

export function newRunId({ track, backend, runIndex, now = new Date(), nonce = randomUUID() }) {
  const stamp = now.toISOString().replace(/[-:.TZ]/g, '').slice(0, 14);
  const safe = value => String(value).toLowerCase().replace(/[^a-z0-9_-]+/g, '-');
  return `${safe(track)}-${safe(backend)}-run${Number(runIndex)}-${stamp}-${safe(nonce).slice(0, 8)}`;
}

export function createBackendLease({ runId, backend, track, runIndex, ownerPid = process.pid,
  serverUri = null, database = null, module = null, dataDir = null, container = null }) {
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
    launchedPid: null,
    listenerPids: [],
  };
  try {
    executeStackLeaseCapability(backend, 'validate-resources', {
      resources,
      helpers: { requireString, loopbackHttpUri },
    });
  } catch (error) {
    if (String(error.message).startsWith('invalid backend lease:')) throw error;
    fail(error.message);
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

export function validateBackendLease(lease, { token, backend, runId, active = false } = {}) {
  if (!lease || typeof lease !== 'object') fail('document is not an object');
  if (lease.version !== LEASE_VERSION) fail(`unsupported version ${lease.version}`);
  requireString(lease.runId, 'runId');
  requireString(lease.backend, 'backend');
  requireString(lease.ownershipToken, 'ownershipToken');
  if (!LEASE_STATES.has(lease.state)) fail(`unknown state ${lease.state}`);
  if (token !== undefined && token !== lease.ownershipToken) fail('ownership token does not match');
  if (backend !== undefined && backend !== lease.backend) fail(`backend is ${lease.backend}, not ${backend}`);
  if (runId !== undefined && runId !== lease.runId) fail(`runId is ${lease.runId}, not ${runId}`);
  if (active && !['active', 'restarting'].includes(lease.state)) fail(`lease is ${lease.state}, not active`);
  try {
    executeStackLeaseCapability(lease.backend, 'validate-resources', {
      resources: lease.resources,
      helpers: { requireString, loopbackHttpUri },
    });
  } catch (error) {
    if (String(error.message).startsWith('invalid backend lease:')) throw error;
    fail(error.message);
  }
  if (lease.resources?.buildContainer != null) {
    requireString(lease.resources.buildContainer.name, 'buildContainer.name');
    requireString(lease.resources.buildContainer.id, 'buildContainer.id');
    requireString(lease.resources.buildContainer.image, 'buildContainer.image');
    if (lease.resources.buildContainer.owned !== true) fail('buildContainer must be benchmark-owned');
    if (lease.resources.buildContainer.networkMode != null
      && !['bridge', 'host'].includes(lease.resources.buildContainer.networkMode)) {
      fail('buildContainer.networkMode is invalid');
    }
  }
  if (!Array.isArray(lease.resources?.locks)) fail('locks must be an array');
  if (lease.resources.launchedPid !== null
    && (!Number.isInteger(lease.resources.launchedPid) || lease.resources.launchedPid <= 0)) {
    fail('launchedPid must be a positive integer or null');
  }
  if (!Array.isArray(lease.resources.listenerPids)
    || lease.resources.listenerPids.some(pid => !/^\d+$/.test(String(pid)) || Number(pid) <= 0)) {
    fail('listenerPids must contain only positive process ids');
  }
  for (const lock of lease.resources.locks) {
    requireString(lock.path, 'lock.path');
    requireString(lock.key, 'lock.key');
    requireString(lock.digest, 'lock.digest');
  }
  return lease;
}

export function readBackendLease(path, expected = {}) {
  requireString(path, 'path');
  let parsed;
  try { parsed = JSON.parse(readFileSync(path, 'utf8')); }
  catch (error) { fail(`cannot read ${path}: ${error.message}`); }
  return validateBackendLease(parsed, expected);
}

export function writeBackendLease(path, lease) {
  validateBackendLease(lease);
  mkdirSync(dirname(path), { recursive: true });
  const temporary = `${path}.${process.pid}.${randomUUID()}.tmp`;
  writeFileSync(temporary, `${JSON.stringify(lease, null, 2)}\n`, { flag: 'wx' });
  renameSync(temporary, path);
}

export function updateBackendLease(path, expected, update) {
  const lease = readBackendLease(path, expected);
  const next = update(structuredClone(lease)) ?? lease;
  validateBackendLease(next, { token: lease.ownershipToken, backend: lease.backend, runId: lease.runId });
  writeBackendLease(path, next);
  return next;
}

export function publicBackendLease(lease) {
  const copy = structuredClone(validateBackendLease(lease));
  copy.ownership = {
    markerSha256: createHash('sha256').update(copy.ownershipToken).digest('hex'),
  };
  delete copy.ownershipToken;
  return copy;
}

const tokenHash = token => createHash('sha256').update(token).digest('hex');

function processAlive(pid) {
  try { process.kill(pid, 0); return true; }
  catch (error) { return error.code === 'EPERM'; }
}

export function acquireResourceLock({ root, key, lease }) {
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
    ownershipMarkerSha256: tokenHash(lease.ownershipToken),
    acquiredAt: new Date().toISOString(),
  };
  for (let attempt = 0; attempt < 4; attempt++) {
    try {
      writeFileSync(path, `${JSON.stringify(record, null, 2)}\n`, { flag: 'wx' });
      return { path, key, digest, acquiredAt: record.acquiredAt };
    } catch (error) {
      if (error.code !== 'EEXIST') throw error;
      let existing;
      try { existing = JSON.parse(readFileSync(path, 'utf8')); }
      catch { fail(`resource lock ${path} exists but is unreadable; refusing to steal it`); }
      if (!Number.isInteger(existing.ownerPid) || !existing.runId || !existing.ownershipMarkerSha256) {
        fail(`resource lock ${path} is malformed; refusing to steal it`);
      }
      if (processAlive(existing.ownerPid)) {
        fail(`resource ${key} is already leased by ${existing.runId} (pid ${existing.ownerPid})`);
      }
      // Move the exact stale inode out of the acquisition path. If another
      // contender won that race, rename fails and the next iteration rereads
      // the new owner's record rather than deleting it.
      const stale = `${path}.stale-${randomUUID()}`;
      try {
        renameSync(path, stale);
        rmSync(stale, { force: true });
      } catch (renameError) {
        if (renameError.code !== 'ENOENT') throw renameError;
      }
    }
  }
  fail(`could not acquire resource ${key} after stale-lock contention`);
}

export function releaseResourceLocks(lease) {
  validateBackendLease(lease);
  for (const lock of lease.resources.locks ?? []) {
    let existing;
    try { existing = JSON.parse(readFileSync(lock.path, 'utf8')); }
    catch (error) {
      if (error.code === 'ENOENT') continue;
      fail(`cannot read resource lock ${lock.path}: ${error.message}`);
    }
    if (existing.runId !== lease.runId
      || existing.ownerPid !== lease.ownerPid
      || existing.ownershipMarkerSha256 !== tokenHash(lease.ownershipToken)) {
      fail(`resource lock ${lock.path} no longer belongs to lease ${lease.runId}`);
    }
    rmSync(lock.path, { force: true });
  }
}

export function leaseFromEnv(env = process.env, expected = {}) {
  const path = requireString(env.STACK_BENCH_LEASE, 'STACK_BENCH_LEASE');
  const token = requireString(env.STACK_BENCH_LEASE_TOKEN, 'STACK_BENCH_LEASE_TOKEN');
  return { path, lease: readBackendLease(path, { ...expected, token }) };
}
