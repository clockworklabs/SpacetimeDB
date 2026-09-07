import { spawnSync } from 'node:child_process';
import type { SpawnSyncOptionsWithStringEncoding } from 'node:child_process';

import { updateBackendLease } from '../src/runtime/backend-lease.js';
import type { BackendLease } from '../src/runtime/backend-lease.js';

interface StoppedBuildContainer {
  id: string;
  running: false;
}

interface LeaseContext {
  path: string;
  lease: BackendLease;
}

export interface DockerExecuteResult {
  status: number | null;
  stdout?: string;
  stderr?: string;
  error?: Error;
}

export type DockerExecute = (command: string, args: readonly string[],
  options: SpawnSyncOptionsWithStringEncoding) => DockerExecuteResult;

export interface RecoverStoppedBuildContainerOptions {
  existing: StoppedBuildContainer;
  containerName: string;
  leaseContext: LeaseContext;
  backend: string;
  dockerEnv?: NodeJS.ProcessEnv;
  timeoutMs?: number;
  execute?: DockerExecute;
}

function clearBuildContainerLease(leaseContext: LeaseContext, backend: string,
  containerId: string, description: string): LeaseContext {
  const lease = updateBackendLease(leaseContext.path, {
    token: leaseContext.lease.ownershipToken, backend, runId: leaseContext.lease.runId,
  }, next => {
    if (next.resources.buildContainer?.id !== containerId) {
      throw new Error(`${description} ownership changed before recovery`);
    }
    next.resources.buildContainer = null;
    return next;
  });
  return { path: leaseContext.path, lease };
}

export function recoverStoppedBuildContainer({ existing, containerName, leaseContext, backend,
  dockerEnv = process.env, timeoutMs = 120_000,
  execute = spawnSync }: RecoverStoppedBuildContainerOptions): LeaseContext {
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
  return clearBuildContainerLease(leaseContext, backend, existing.id, 'stopped container');
}

export function clearMissingBuildContainerLease({ containerName, leaseContext, backend }: {
  containerName: string; leaseContext: LeaseContext; backend: string;
}): LeaseContext {
  const prior = leaseContext?.lease?.resources?.buildContainer ?? null;
  if (!prior || prior.name !== containerName) {
    throw new Error('missing container does not match the authenticated lease');
  }
  return clearBuildContainerLease(leaseContext, backend, prior.id, 'missing container');
}
