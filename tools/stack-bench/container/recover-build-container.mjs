import { spawnSync } from 'node:child_process';

import { updateBackendLease } from '../src/runtime/backend-lease.mjs';

export function recoverStoppedBuildContainer({ existing, containerName, leaseContext, backend,
  dockerEnv = process.env, timeoutMs = 120_000, execute = spawnSync }) {
  const prior = leaseContext?.lease?.resources?.buildContainer ?? null;
  if (!existing || existing.running) throw new Error('recovery requires a stopped container');
  if (!prior || prior.name !== containerName || prior.id !== existing.id) {
    throw new Error('stopped container does not match the authenticated lease');
  }
  const removed = execute('docker', ['rm', existing.id], {
    encoding: 'utf8', env: dockerEnv, timeout: timeoutMs,
  });
  if (removed.status !== 0) {
    throw new Error(`could not remove exact stopped leased container ${existing.id}: `
      + String(removed.stderr || removed.stdout || removed.error?.message || `exit ${removed.status}`).trim());
  }
  const lease = updateBackendLease(leaseContext.path, {
    token: leaseContext.lease.ownershipToken, backend, runId: leaseContext.lease.runId,
  }, next => {
    if (next.resources.buildContainer?.id !== existing.id) {
      throw new Error('stopped container ownership changed before recovery');
    }
    next.resources.buildContainer = null;
    return next;
  });
  return { path: leaseContext.path, lease };
}
