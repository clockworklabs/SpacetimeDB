// Called only under the persistent kernel flock in backend-lease.ts.
import { createHash, randomUUID } from 'node:crypto';
import { closeSync, existsSync, fsyncSync, linkSync, openSync, readFileSync,
  rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { processIdentity } from './platform.js';
import type { BackendLease, BackendResourceLock } from './backend-lease.js';

export interface ResourceLockTransaction {
  operation: 'acquire' | 'release' | 'release-intent' | 'verify';
  root: string;
  lease: BackendLease;
  keys: string[];
}

const hash = (value: string): string => createHash('sha256').update(value).digest('hex');
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

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
export function resourceLockTransaction(input: ResourceLockTransaction): BackendResourceLock[] {
  const { operation, root, lease, keys } = input;
  const locks = resourceLockDescriptors(root, keys);
  const owned = (record: Record<string, unknown>): boolean => record.runId === lease.runId
    && record.ownerPid === lease.ownerPid
    && record.ownershipMarkerSha256 === hash(lease.ownershipToken);
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
    if (!record || owned(record)) continue;
    const identity = processIdentity(record.ownerPid as number);
    if (identity && (record.ownerStartMarker === null
      || record.ownerStartMarker === identity.startMarker)) {
      throw new Error(`resource ${lock.key} is already leased by ${record.runId} (pid ${record.ownerPid})`);
    }
    // Controller PID namespaces differ. Missing local PIDs cannot authorize reuse.
    throw new Error(`resource ${lock.key} remains leased by ${record.runId}; run authenticated recovery before reuse`);
  }
  const acquired: BackendResourceLock[] = [];
  try {
    for (const [index, lock] of locks.entries()) {
      const record = existing[index];
      if (operation === 'release' || (operation === 'release-intent' && record && owned(record))) {
        rmSync(lock.path, { force: true });
      } else if (operation === 'acquire' && !record) {
        const acquiredAt = new Date().toISOString();
        const temporary = `${lock.path}.${randomUUID()}.tmp`;
        const fd = openSync(temporary, 'wx', 0o600);
        try {
          writeFileSync(fd, `${JSON.stringify({ version: 1, key: lock.key,
            runId: lease.runId, ownerPid: lease.ownerPid,
            ownerStartMarker: processIdentity(lease.ownerPid)?.startMarker ?? null,
            ownershipMarkerSha256: hash(lease.ownershipToken), acquiredAt })}\n`);
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
