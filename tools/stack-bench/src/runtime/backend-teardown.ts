// Idempotent, lease-authenticated backend teardown shared by the benchmark and
// its outer qualification supervisor.

import { execFileSync } from 'node:child_process';
import { readBackendLease, releaseResourceLocks, updateBackendLease } from './backend-lease.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';

const DOCKER_TIMEOUT_MS = 120_000;
const REMOVE_RETRY_DELAYS_MS = [0, 250, 750] as const;

function errorField(error: unknown, field: 'stderr' | 'message'): unknown {
  return typeof error === 'object' && error !== null ? Reflect.get(error, field) : undefined;
}

function dockerMissing(error: unknown): boolean {
  return /No such (object|container)/i.test(
    `${errorField(error, 'stderr') ?? ''}${errorField(error, 'message') ?? ''}`,
  );
}

function wait(milliseconds: number): void {
  if (milliseconds <= 0) return;
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, milliseconds);
}

export interface DockerTeardownOperations {
  inspect(name: string): string;
  remove(id: string): void;
  wait(milliseconds: number): void;
}

const DOCKER: DockerTeardownOperations = {
  inspect(name) {
    return execFileSync('docker', ['inspect', '--format', '{{.Id}}', name], {
      encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS,
    }).trim();
  },
  remove(id) {
    execFileSync('docker', ['rm', '-f', id], {
      stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS,
      env: { ...process.env, MSYS_NO_PATHCONV: '1' },
    });
  },
  wait,
};

export function stopLeasedContainer(leasePath: string, leaseToken: string,
  docker: DockerTeardownOperations = DOCKER): boolean {
  const lease = readBackendLease(leasePath, { token: leaseToken });
  const container = lease.resources.buildContainer;
  if (!container) return true;
  let actual = null;
  try {
    actual = docker.inspect(container.name);
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
    let removed = false;
    for (const delay of REMOVE_RETRY_DELAYS_MS) {
      docker.wait(delay);
      try {
        docker.remove(container.id);
        removed = true;
        break;
      } catch (error) {
        if (dockerMissing(error)) {
          removed = true;
          break;
        }
      }
    }
    if (!removed) {
      console.error(`  REFUSED to release lease: Docker could not remove ${container.name} after ${REMOVE_RETRY_DELAYS_MS.length} attempts`);
      return false;
    }
    console.log(`  removed the leased run container ${container.name}`);
  }
  updateBackendLease(leasePath,
    { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
      next.resources.buildContainer!.running = false;
      next.resources.buildContainer!.removedAt ??= new Date().toISOString();
      return next;
    });
  return true;
}

export function releaseBackendLease(
  leasePath: string,
  leaseToken: string,
  { retainBackend = false }: { retainBackend?: boolean } = {},
): boolean {
  let lease = readBackendLease(leasePath, { token: leaseToken });
  if (lease.state === 'released') return true;
  let released = stopLeasedContainer(leasePath, leaseToken);
  released = STACK_ADAPTER_REGISTRY.get(lease.backend).teardown.host({
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
