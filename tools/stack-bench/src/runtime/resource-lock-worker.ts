// Called only under the persistent kernel flock in backend-lease.ts.
import { createHash, randomUUID } from 'node:crypto';
import { closeSync, existsSync, fsyncSync, linkSync, openSync, readFileSync,
  rmSync, writeFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { cpus, loadavg } from 'node:os';
import { processIdentity } from './platform.js';
import { ATTEMPT_STARTUP_MEMORY_BYTES } from '../composition/product-config.js';
import type { BackendLease, BackendResourceLock } from './backend-lease.js';

export interface ResourceLockTransaction {
  operation: 'acquire' | 'release' | 'release-intent' | 'verify';
  root: string;
  lease: BackendLease;
  keys: string[];
  capacity?: number | null;
}

const hash = (value: string): string => createHash('sha256').update(value).digest('hex');
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function slotIdentity(key: unknown, owner: unknown): string | null {
  const slot = typeof key === 'string' ? /^slot:([^:]+):[^:]+:run(\d+)$/.exec(key) : null;
  return slot ? `${owner}:${slot[1]}:${slot[2]}` : null;
}

function addSlot(slots: Set<string>, key: unknown, owner: unknown): void {
  const slot = slotIdentity(key, owner);
  if (slot) slots.add(slot);
}

export interface HostPressure {
  readonly memTotalBytes: number;
  readonly memAvailableBytes: number;
  readonly cpuCount: number;
  readonly load: number;
}

const memoryFloor = (total: number): number => Math.max(2 * 1024 ** 3, total * 0.1);

const validHostPressure = ({ memTotalBytes: total, memAvailableBytes: available, cpuCount, load }: HostPressure):
  boolean => [total, available, cpuCount, load].every(Number.isFinite)
    && total > 0 && available >= 0 && available <= total && cpuCount > 0 && load >= 0;

export function hostResourceWaitReason(total: number, available: number, cpuCount: number, load: number,
  startingAttempts = 1, startingMemoryBytes = startingAttempts * ATTEMPT_STARTUP_MEMORY_BYTES): string | null {
  if (!validHostPressure({ memTotalBytes: total, memAvailableBytes: available, cpuCount, load })
    || !Number.isSafeInteger(startingAttempts) || startingAttempts < 1
    || !Number.isSafeInteger(startingMemoryBytes) || startingMemoryBytes <= 0) {
    throw new Error('cannot read valid host resource pressure');
  }
  const required = memoryFloor(total) + startingMemoryBytes;
  if (available < required) return `host capacity unavailable: ${(available / 1024 ** 3).toFixed(1)} GiB available; ${(required / 1024 ** 3).toFixed(1)} GiB required before another attempt`;
  if (load >= cpuCount) return `host capacity unavailable: CPU load ${load.toFixed(1)} on ${cpuCount} CPUs`;
  return null;
}

// The kernel this process runs under. In the controller container that is the Docker VM.
export function readHostPressure(): HostPressure {
  const mem = readFileSync('/proc/meminfo', 'utf8');
  const pressure = { memTotalBytes: Number(mem.match(/^MemTotal:\s+(\d+)/m)?.[1]) * 1024,
    memAvailableBytes: Number(mem.match(/^MemAvailable:\s+(\d+)/m)?.[1]) * 1024,
    cpuCount: cpus().length, load: loadavg()[0]! };
  if (!validHostPressure(pressure)) throw new Error('cannot read valid host resource pressure');
  return pressure;
}

// Admission's floor with no attempt starting: CPU saturated or memory below its reserve.
export const hostOverCapacity = (pressure: HostPressure): boolean =>
  pressure.load >= pressure.cpuCount || pressure.memAvailableBytes < memoryFloor(pressure.memTotalBytes);

function readHostResourceWaitReason(startingAttempts: number, startingMemoryBytes: number): string | null {
  const { memTotalBytes, memAvailableBytes, cpuCount, load } = readHostPressure();
  return hostResourceWaitReason(memTotalBytes, memAvailableBytes, cpuCount, load, startingAttempts, startingMemoryBytes);
}

export function resourceLockDescriptors(root: string, keys: string[]): BackendResourceLock[] {
  return [...new Set(keys)].sort().map(key => {
    const digest = hash(key);
    return { path: resolve(root, `${digest}.lock.json`), key, digest };
  });
}

function syncDirectory(root: string): void {
  if (process.platform !== 'linux') return;
  const fd = openSync(root, 'r');
  try { fsyncSync(fd); } finally { closeSync(fd); }
}

// This function has no locking of its own. Production callers must use flock.
// Keeping the transaction separate lets source tests exercise records on Windows.
export function resourceLockTransaction(input: ResourceLockTransaction,
  readPressure: (startingAttempts: number, startingMemoryBytes: number) => string | null = readHostResourceWaitReason,
  now = Date.now()): BackendResourceLock[] {
  const { operation, root, lease, keys } = input;
  const locks = resourceLockDescriptors(root, keys);
  const owned = (record: Record<string, unknown>): boolean => record.runId === lease.runId
    && record.ownerPid === lease.ownerPid
    && record.ownershipMarkerSha256 === hash(lease.ownershipToken);
  if (operation === 'acquire' && input.capacity != null) {
    if (!Number.isSafeInteger(input.capacity) || input.capacity < 0) throw new Error('invalid runner capacity');
    const slots = new Set<string>();
    for (const name of readdirSync(root).filter(name => name.endsWith('.lock.json'))) {
      const record: unknown = JSON.parse(readFileSync(resolve(root, name), 'utf8'));
      if (!object(record) || typeof record.key !== 'string'
        || typeof record.ownershipMarkerSha256 !== 'string') throw new Error('unreadable host resource claim');
      addSlot(slots, record.key, record.ownershipMarkerSha256);
    }
    for (const key of keys) addSlot(slots, key, hash(lease.ownershipToken));
    if (slots.size > input.capacity) throw new Error(`host capacity unavailable: ${slots.size} reservations exceed capacity ${input.capacity}`);
  }
  const existing = locks.map(lock => {
    if (!existsSync(lock.path)) return null;
    let record: unknown;
    try { record = JSON.parse(readFileSync(lock.path, 'utf8')); }
    catch { throw new Error(`resource lock ${lock.path} exists but is unreadable; refusing to steal it`); }
    if (!object(record) || !Number.isInteger(record.ownerPid)
      || typeof record.ownerPid !== 'number' || typeof record.runId !== 'string'
      || !record.runId || typeof record.ownershipMarkerSha256 !== 'string'
      || !record.ownershipMarkerSha256) {
      throw new Error(`resource lock ${lock.path} is malformed; refusing to steal it`);
    }
    return record;
  });
  for (const [index, lock] of locks.entries()) {
    const record = existing[index];
    if (operation === 'release-intent') continue;
    if (operation !== 'acquire') {
      if ((!record && operation === 'verify') || (record && !owned(record))) {
        throw new Error(`resource lock ${lock.path} no longer belongs to lease ${lease.runId}`);
      }
      continue;
    }
    if (!record) continue;
    if (owned(record)) continue;
    const identity = processIdentity(record.ownerPid as number);
    if (identity && (record.ownerStartMarker === null
      || record.ownerStartMarker === identity.startMarker)) {
      throw new Error(`resource ${lock.key} is already leased by ${record.runId} (pid ${record.ownerPid})`);
    }
    // Controller PID namespaces differ. Missing local PIDs cannot authorize reuse.
    throw new Error(`resource ${lock.key} remains leased by ${record.runId}; run authenticated recovery before reuse`);
  }
  const acquired: BackendResourceLock[] = [];
  if (operation === 'acquire' && input.capacity === null
    && locks.some((lock, index) => lock.key.startsWith('slot:') && !existing[index])) {
    const starting = new Map<string, number>();
    const reserve = (key: unknown, owner: unknown, memoryBytes: number) => {
      const slot = slotIdentity(key, owner);
      if (slot) starting.set(slot, Math.max(starting.get(slot) ?? 0, memoryBytes));
    };
    // Hold the measured startup reservation for each launch's first minute, then
    // use measured pressure; phase reservations would be needed to guarantee later spikes.
    for (const name of readdirSync(root).filter(name => name.endsWith('.lock.json'))) {
      const record: unknown = JSON.parse(readFileSync(resolve(root, name), 'utf8'));
      if (!object(record) || typeof record.key !== 'string'
        || typeof record.ownershipMarkerSha256 !== 'string' || typeof record.acquiredAt !== 'string'
        || !Number.isFinite(Date.parse(record.acquiredAt))) throw new Error('unreadable host resource claim');
      const memoryBytes = record.startupMemoryBytes ?? ATTEMPT_STARTUP_MEMORY_BYTES;
      if (typeof memoryBytes !== 'number' || !Number.isSafeInteger(memoryBytes) || memoryBytes <= 0) {
        throw new Error('unreadable host resource claim startup memory');
      }
      if (now - Date.parse(record.acquiredAt) < 60_000) reserve(record.key, record.ownershipMarkerSha256,
        memoryBytes);
    }
    for (const key of keys) reserve(key, hash(lease.ownershipToken), ATTEMPT_STARTUP_MEMORY_BYTES);
    const reason = readPressure(starting.size, [...starting.values()].reduce((sum, value) => sum + value, 0));
    if (reason) throw new Error(reason);
  }
  try {
    for (const [index, lock] of locks.entries()) {
      const record = existing[index];
      if (operation === 'release' || (operation === 'release-intent' && record && owned(record))) {
        rmSync(lock.path, { force: true });
      } else if (operation === 'acquire' && !record) {
        const acquiredAt = new Date(now).toISOString();
        const temporary = `${lock.path}.${randomUUID()}.tmp`;
        const fd = openSync(temporary, 'wx', 0o600);
        try {
          writeFileSync(fd, `${JSON.stringify({ version: 1, key: lock.key,
            runId: lease.runId, ownerPid: lease.ownerPid,
            ownerStartMarker: processIdentity(lease.ownerPid)?.startMarker ?? null,
            ownershipMarkerSha256: hash(lease.ownershipToken), acquiredAt,
            startupMemoryBytes: ATTEMPT_STARTUP_MEMORY_BYTES })}\n`);
          fsyncSync(fd);
        } finally { closeSync(fd); }
        try { linkSync(temporary, lock.path); }
        finally { rmSync(temporary, { force: true }); }
        acquired.push(lock);
        lock.acquiredAt = acquiredAt;
      } else if (record && typeof record.acquiredAt === 'string') {
        lock.acquiredAt = record.acquiredAt;
      }
    }
    syncDirectory(root);
    return locks;
  } catch (error) {
    // A process crash leaves the complete claim set recoverable by private intent.
    // Ordinary failures roll back only claims made by this transaction.
    for (const lock of acquired) rmSync(lock.path, { force: true });
    syncDirectory(root);
    throw error;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const input = JSON.parse(readFileSync(0, 'utf8')) as ResourceLockTransaction;
    process.stdout.write(JSON.stringify(resourceLockTransaction(input)));
  } catch (error) {
    process.stderr.write(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
