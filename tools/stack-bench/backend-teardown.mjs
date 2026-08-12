// Idempotent, lease-authenticated backend teardown shared by the benchmark and
// its outer qualification supervisor.

import { execFileSync } from 'node:child_process';
import { readBackendLease, releaseResourceLocks, updateBackendLease } from './backend-lease.mjs';
import { executeStackCapability } from './stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.mjs';

const DOCKER_TIMEOUT_MS = 120_000;

function dockerMissing(error) {
  return /No such (object|container)/i.test(`${error?.stderr ?? ''}${error?.message ?? ''}`);
}

export function stopLeasedContainer(leasePath, leaseToken) {
  const lease = readBackendLease(leasePath, { token: leaseToken });
  const container = lease.resources.buildContainer;
  if (!container || container.running === false) return true;
  let actual = null;
  try {
    actual = execFileSync('docker', ['inspect', '--format', '{{.Id}}', container.name], {
      encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS,
    }).trim();
  } catch (error) {
    if (!dockerMissing(error)) {
      console.error(`  REFUSED to assume container ${container.name} is gone: Docker inspection failed`);
      return false;
    }
  }
  if (actual && actual !== container.id) {
    console.error(`  REFUSED to remove container ${container.name}: id ${actual} does not match lease ${container.id}`);
    return false;
  }
  if (actual) {
    try {
      execFileSync('docker', ['rm', '-f', container.id], {
        stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS,
        env: { ...process.env, MSYS_NO_PATHCONV: '1' },
      });
      console.log(`  removed the leased run container ${container.name}`);
    } catch {
      console.error(`  REFUSED to release lease: Docker could not remove ${container.name}`);
      return false;
    }
  }
  updateBackendLease(leasePath,
    { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
      next.resources.buildContainer.running = false;
      next.resources.buildContainer.removedAt ??= new Date().toISOString();
      return next;
    });
  return true;
}

export function releaseBackendLease(leasePath, leaseToken, { retainBackend = false } = {}) {
  let lease = readBackendLease(leasePath, { token: leaseToken });
  if (lease.state === 'released') return true;
  let released = stopLeasedContainer(leasePath, leaseToken);
  released = executeStackCapability(STACK_ADAPTER_REGISTRY.get(lease.backend),
    'teardown', 'host', {
      leasePath, leaseToken, lease, retainHost: retainBackend,
    }) && released;
  if (!released || retainBackend) return released;
  lease = readBackendLease(leasePath,
    { token: leaseToken, backend: lease.backend, runId: lease.runId });
  releaseResourceLocks(lease);
  updateBackendLease(leasePath,
    { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
      for (const lock of next.resources.locks) lock.releasedAt ??= new Date().toISOString();
      next.state = 'released';
      next.releasedAt ??= new Date().toISOString();
      return next;
    });
  return true;
}
