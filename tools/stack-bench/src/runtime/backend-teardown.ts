// Idempotent, lease-authenticated backend teardown shared by the benchmark and
// its outer qualification supervisor.

import { execFileSync } from 'node:child_process';
import { readBackendLease, releaseResourceLocks, updateBackendLease } from './backend-lease.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';
import { ATTEMPT_CREATION_LABEL, attemptDocker } from './docker-network.js';
import type { BackendCreationKind } from './backend-lease.js';
import type { TextCommandExecutor } from './command-executor.js';
import { CODING_CONTAINER_AGENT, codingContainerWorkspaceHandoffCommands } from './coding-container-policy.js';

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

export function dockerNetworkMissing(error: unknown): boolean {
  return /No such network|network \S+ not found/i.test(
    `${errorField(error, 'stderr') ?? ''}\n${errorField(error, 'message') ?? ''}`,
  );
}

function wait(milliseconds: number): void {
  if (milliseconds <= 0) return;
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, milliseconds);
}

export interface DockerTeardownOperations {
  inspect(name: string): string;
  handoffWorkspace(id: string): void;
  remove(id: string): void;
  wait(milliseconds: number): void;
}

export function handoffBuildWorkspace(id: string, exec: TextCommandExecutor = execFileSync): void {
  const runtime = JSON.parse(exec('docker', ['inspect', '--format',
    '{"state":{{json .State}},"pidMode":{{json .HostConfig.PidMode}}}', id],
  { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS })) as {
    state: { Status: string; Running: boolean }; pidMode: string;
  };
  // A container that never started has no application writers or generated files.
  if (runtime.state.Status === 'created') return;
  if (!runtime.state.Running) throw new Error(`workspace handback requires running build container ${id}; `
    + 'the stopped container and private lease must be retained for operator recovery');
  if (runtime.pidMode !== '' && runtime.pidMode !== 'private') {
    throw new Error(`workspace handback requires a private PID namespace for ${id}`);
  }
  const uid = CODING_CONTAINER_AGENT.uid;
  const stopWriters = `pkill -KILL -u ${uid} || [ "$?" = 1 ]; attempt=0; `
    + `while ps -u ${uid} -o stat= | grep -qv '^[[:space:]]*Z'; do `
    + '[ "$attempt" -lt 50 ] || { echo "application writers did not stop" >&2; exit 1; }; '
    + `pkill -KILL -u ${uid} || [ "$?" = 1 ]; attempt=$((attempt + 1)); sleep 0.1; done`;
  for (const command of [['sh', '-ec', stopWriters],
    ...codingContainerWorkspaceHandoffCommands(process.getgid?.() ?? 0)]) {
    exec('docker', ['exec', '--user', '0:0', id, ...command],
      { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
  }
}

const DOCKER: DockerTeardownOperations = {
  inspect(name) {
    return execFileSync('docker', ['inspect', '--format', '{{.Id}}', name], {
      encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS,
    }).trim();
  },
  handoffWorkspace: handoffBuildWorkspace,
  remove(id) {
    execFileSync('docker', ['rm', '-f', '--volumes', id], {
      stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS,
      env: { ...process.env, MSYS_NO_PATHCONV: '1' },
    });
  },
  wait,
};

export function stopLeasedContainer(leasePath: string, leaseToken: string,
  docker: DockerTeardownOperations = DOCKER,
  key: 'buildContainer' | 'browserContainer' | 'brokerContainer' | 'smokeContainer' | 'container' = 'buildContainer'): boolean {
  const lease = readBackendLease(leasePath, { token: leaseToken });
  const container = lease.resources[key];
  if (!container) return true;
  if (container.owned !== true) return true;
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
  if (key === 'buildContainer' && !actual && !container.removedAt && !container.workspaceHandedBackAt) {
    throw new Error(`build container ${container.id} disappeared before workspace handback was recorded; `
      + 'retain the private lease and inspect workspace ownership before operator recovery');
  }
  if (actual) {
    if (key === 'buildContainer' && !container.workspaceHandedBackAt) {
      docker.handoffWorkspace(container.id);
      // Persist before removal so recovery can finish after the container is gone.
      updateBackendLease(leasePath, { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
        next.resources.buildContainer!.workspaceHandedBackAt = new Date().toISOString();
        return next;
      });
    }
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
      next.resources[key]!.running = false;
      next.resources[key]!.removedAt ??= new Date().toISOString();
      return next;
    });
  return true;
}

function removeAttemptNetwork(leasePath: string, leaseToken: string): boolean {
  const lease = readBackendLease(leasePath, { token: leaseToken });
  // Creation authority covers death between Docker create and recording its ID.
  const order: BackendCreationKind[] = ['broker', 'browser', 'smoke', 'build', 'firewall', 'backend', 'network'];
  for (const kind of order) {
    const intent = lease.resources.creationIntents?.[kind];
    if (!intent) continue;
    let resource;
    try {
      resource = JSON.parse(attemptDocker(kind === 'network'
        ? ['network', 'inspect', intent.name] : ['container', 'inspect', intent.name]))[0];
    } catch (error) {
      if (dockerMissing(error) || (kind === 'network' && dockerNetworkMissing(error))) continue;
      throw error;
    }
    const label = kind === 'network' ? resource.Labels?.[ATTEMPT_CREATION_LABEL]
      : resource.Config?.Labels?.[ATTEMPT_CREATION_LABEL];
    if (label !== intent.creationToken) throw new Error(`refusing cleanup of ${kind}: creation authority changed`);
    if (kind === 'network') {
      const cache = lease.resources.network?.cacheContainerId;
      if (cache && resource.Containers?.[cache]) attemptDocker(['network', 'disconnect', resource.Id, cache]);
      attemptDocker(['network', 'rm', resource.Id]);
    } else attemptDocker(['rm', '-f', '--volumes', resource.Id]);
  }
  return true;
}

export function releaseBackendLease(
  leasePath: string,
  leaseToken: string,
  { retainBackend = false }: { retainBackend?: boolean } = {},
): boolean {
  let lease = readBackendLease(leasePath, { token: leaseToken });
  if (lease.state === 'released') return true;
  let released = true;
  for (const key of ['brokerContainer', 'browserContainer', 'smokeContainer', 'buildContainer'] as const) {
    released = stopLeasedContainer(leasePath, leaseToken, DOCKER, key) && released;
  }
  if (lease.resources.network && !retainBackend && released) {
    released = stopLeasedContainer(leasePath, leaseToken, DOCKER, 'container') && released;
    if (released) released = removeAttemptNetwork(leasePath, leaseToken);
  } else if (!retainBackend && released && lease.resources.creationIntents) {
    released = removeAttemptNetwork(leasePath, leaseToken);
  }
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
