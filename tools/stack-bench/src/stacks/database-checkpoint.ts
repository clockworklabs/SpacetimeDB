import { execFileSync } from 'node:child_process';
import { closeSync, createReadStream, openSync } from 'node:fs';
import { createHash } from 'node:crypto';

import type { BackendLease } from '../runtime/backend-lease.js';
import { requireAttemptNetwork, attemptDocker } from '../runtime/docker-network.js';
import { assertLeasedContainer } from './backend-reset-guard.js';
import { hostedRecordedProcessStopScript, SPACETIME_PROCESS_RECORD,
  startAttemptDatabaseProcess } from './hosted-lifecycle.js';
import { attemptDatabaseIdentity } from './hosted-database-identity.js';
import { waitFor } from './lifecycle-readiness.js';

// Cold checkpoints are same-lease only. They include native schema, IDs, and
// credentials. Keep them outside the coding workspace and public exports.
export function checkpointDatabaseTarget(lease: BackendLease): { container: string; data: string } {
  if (process.platform !== 'linux') throw new Error('database checkpoints require the Linux appliance');
  requireAttemptNetwork(lease);
  const container = lease.resources.container;
  if (!container?.owned) throw new Error('database checkpoint requires an owned container');
  const data = lease.backend === 'postgres' ? '/var/lib/postgresql/data'
    : lease.backend === 'mongodb' ? '/data/db'
    : lease.backend === 'spacetime' ? '/var/lib/stack-bench-data' : null;
  if (!data) throw new Error('unsupported database checkpoint backend');
  return { container: assertLeasedContainer(container, execFileSync, 30_000, 'database checkpoint'), data };
}

export async function checkpointFileHash(path: string): Promise<string> {
  const hash = createHash('sha256');
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest('hex');
}

export function stopCheckpointDatabase(lease: BackendLease): void {
  const { container, data } = checkpointDatabaseTarget(lease);
  const command = lease.backend === 'postgres'
    ? ['-u', 'postgres', container, 'pg_ctl', '-D', data, '-m', 'fast', '-w', '-t', '60', 'stop']
    : lease.backend === 'mongodb'
    ? ['-u', 'mongodb', container, 'mongod', '--shutdown', '--dbpath', data]
    : [container, 'sh', '-c', hostedRecordedProcessStopScript(SPACETIME_PROCESS_RECORD)];
  execFileSync('docker', ['exec', ...command], { stdio: 'pipe', timeout: 90_000 });
  // Refuse to archive/replace files if a database process is still alive. The
  // namespace anchor remains running; restarting it would break the lease.
  const executable = lease.backend === 'postgres' ? 'postgres'
    : lease.backend === 'mongodb' ? 'mongod' : 'spacetimedb';
  execFileSync('docker', ['exec', container, 'sh', '-c',
    'for f in /proc/[0-9]*/comm; do read name < "$f" || continue; '
      + 'if [ "$name" = "$1" ]; then echo "database process still alive" >&2; exit 1; fi; done',
    'sh', executable], { stdio: 'pipe', timeout: 30_000 });
}

export async function startCheckpointDatabase(lease: BackendLease): Promise<void> {
  const { container } = checkpointDatabaseTarget(lease);
  startAttemptDatabaseProcess(lease);
  const credentials = attemptDatabaseIdentity(lease.ownershipToken);
  await waitFor(async () => {
    try {
      if (lease.backend === 'postgres') attemptDocker(['exec', container, 'pg_isready', '-U', 'postgres']);
      else if (lease.backend === 'mongodb') attemptDocker(['exec', container, 'mongosh', '--quiet',
        '--username', 'admin', '--password', credentials.adminPassword, '--authenticationDatabase', 'admin',
        '--eval', 'if (!db.hello().isWritablePrimary) quit(1)']);
      else attemptDocker(['exec', container, 'node', '-e',
        `fetch('http://127.0.0.1:${Number(new URL(lease.resources.serverUri!).port)}/v1/ping')`
          + '.then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))']);
      return true;
    } catch { return false; }
  }, 90_000, 'checkpoint database to start');
}

// Call only after stopCheckpointDatabase. Data paths are fixed above, never
// supplied by a receipt or app. An error leaves the database stopped.
export function copyCheckpointDatabase(lease: BackendLease, archive: string, restore: boolean): void {
  const { container, data } = checkpointDatabaseTarget(lease);
  const descriptor = openSync(archive, restore ? 'r' : 'wx', 0o600);
  try {
    const script = restore
      ? 'set -eu; test -d "$1"; test ! -L "$1"; find "$1" -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +; tar -xpf - -C "$1"'
      : 'set -eu; test -d "$1"; test ! -L "$1"; tar -cpf - -C "$1" .';
    execFileSync('docker', ['exec', ...(restore ? ['-i'] : []), container, 'sh', '-c', script, 'sh', data],
      { timeout: 600_000, stdio: restore ? [descriptor, 'pipe', 'pipe'] : ['ignore', descriptor, 'pipe'] });
  } finally { closeSync(descriptor); }
}
