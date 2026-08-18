import { createHash, randomUUID } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { hostname } from 'node:os';
import { join, resolve } from 'node:path';

const VERSION = 2;
const HASH = /^[a-f0-9]{64}$/;
const SAFE_ID = /^[a-z0-9][a-z0-9.-]*$/;
const CONTAINER_HOSTNAME_MOUNT = /\/containers\/([a-f0-9]{64})\/hostname(?=\s|$)/;
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const tokenHash = token => createHash('sha256').update(token).digest('hex');
const fail = message => { throw new Error(`campaign lock: ${message}`); };

function processAlive(pid) {
  try { process.kill(pid, 0); return true; }
  catch (error) { return error.code === 'EPERM'; }
}

export function controllerInstance(env = process.env, {
  readMountInfo = () => readFileSync('/proc/self/mountinfo', 'utf8'),
  fallbackHostname = hostname,
} = {}) {
  const explicit = env.STACK_BENCH_CONTROLLER_INSTANCE?.trim();
  if (explicit) return explicit;
  try {
    const containerId = readMountInfo().match(CONTAINER_HOSTNAME_MOUNT)?.[1];
    if (containerId) return containerId;
  } catch { /* /proc is unavailable outside Linux containers */ }
  return fallbackHostname();
}

function ownerAlive(record, currentInstance) {
  if (record.ownerInstance === currentInstance) return processAlive(record.ownerPid);
  try {
    return execFileSync('docker', ['inspect', '--format', '{{.State.Running}}', record.ownerInstance],
      { encoding: 'utf8', stdio: 'pipe', timeout: 15_000 }).trim() === 'true';
  } catch {
    return false;
  }
}

function validateRecord(input, path = '<campaign-lock>') {
  if (!object(input)) fail(`${path} must contain an object`);
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
  if (!Number.isInteger(input.ownerPid) || input.ownerPid <= 0) fail(`${path}.ownerPid is invalid`);
  if (typeof input.ownerInstance !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9_.-]{0,127}$/.test(input.ownerInstance)) {
    fail(`${path}.ownerInstance is invalid`);
  }
  if (typeof input.acquiredAt !== 'string' || Number.isNaN(Date.parse(input.acquiredAt))) {
    fail(`${path}.acquiredAt is invalid`);
  }
  return structuredClone(input);
}

function readRecord(path) {
  let parsed;
  try { parsed = JSON.parse(readFileSync(path, 'utf8')); }
  catch (error) {
    if (error.code === 'ENOENT') return null;
    fail(`${path} is unreadable; refusing to steal it`);
  }
  return validateRecord(parsed, path);
}

export function acquireCampaignLock(directory, campaign,
  { ownerPid = process.pid, ownerInstance = controllerInstance(), now = new Date().toISOString(), alive = ownerAlive,
    uuid = randomUUID } = {}) {
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
      if (error.code !== 'EEXIST') throw error;
      const existing = readRecord(path);
      if (alive(existing, ownerInstance)) {
        fail(`${campaign.id} is already controlled by ${existing.ownerInstance} pid ${existing.ownerPid}`);
      }
      const stale = `${path}.stale-${uuid()}`;
      try {
        renameSync(path, stale);
        rmSync(stale, { force: true });
      } catch (renameError) {
        if (renameError.code !== 'ENOENT') throw renameError;
      }
    }
  }
  fail(`could not acquire ${campaign.id} after stale-lock contention`);
}

export function releaseCampaignLock(lock) {
  if (!object(lock) || typeof lock.path !== 'string' || typeof lock.token !== 'string'
    || !object(lock.record)) fail('release requires the acquired lock handle');
  const releasing = `${lock.path}.releasing-${randomUUID()}`;
  try { renameSync(lock.path, releasing); }
  catch (error) {
    if (error.code === 'ENOENT') return false;
    throw error;
  }
  const existing = readRecord(releasing);
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
