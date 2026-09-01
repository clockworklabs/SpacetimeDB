import { spawn } from 'node:child_process';
import type { ChildProcess } from 'node:child_process';
import { existsSync, mkdirSync } from 'node:fs';

import { updateBackendLease } from '../runtime/backend-lease.js';
import { answers as answersSync, killDetachedTree, killTree, pidsOnPort, processIdentity,
  processIdentityMatches, sleepSync } from '../runtime/platform.js';
import type { BackendLease } from '../runtime/backend-lease.js';
import { answers, waitFor } from './lifecycle-readiness.js';

const leaseDataDir = (dataDir: string | null | undefined): string => {
  if (!dataDir) throw new Error('SpacetimeDB lease records no data directory');
  return dataDir;
};

const leaseUrl = (serverUri: string | null): URL => {
  if (!serverUri) throw new Error('SpacetimeDB lease records no server URI');
  return new URL(serverUri);
};

export async function controlSpacetime({ lease, signal = null }: {
  lease: BackendLease; signal?: AbortSignal | null;
}): Promise<void> {
  const leasePath = process.env.STACK_BENCH_LEASE ?? '';
  const url = leaseUrl(lease.resources.serverUri);
  const port = Number(url.port);
  const cli = process.env.SPACETIME_BIN;
  if (!cli || !existsSync(cli)) throw new Error(`SPACETIME_BIN is unavailable: ${cli ?? '<unset>'}`);
  const actual = pidsOnPort(port);
  const unexpected = actual.filter(pid => !lease.resources.listenerProcesses.some(identity =>
    identity.pid === Number(pid) && processIdentityMatches(identity)));
  if (unexpected.length) throw new Error(`listener ${unexpected.join(',')} is not owned by lease ${lease.runId}`);
  updateBackendLease(leasePath,
    { token: lease.ownershipToken, backend: 'spacetime', runId: lease.runId }, next => {
      next.state = 'restarting'; return next;
    });
  for (const pid of actual) killTree(pid);
  await waitFor(async () => !(await answers(`${lease.resources.serverUri}/v1/ping`)),
    30_000, 'SpacetimeDB to stop', signal);
  if (process.env.STACK_BENCH_TEST_FAIL_AFTER_RESTART_STOP === '1') {
    throw Object.assign(new Error('injected failure after restart stop'),
      { code: 'injected_restart_stop_failure' });
  }
  updateBackendLease(leasePath,
    { token: lease.ownershipToken, backend: 'spacetime', runId: lease.runId }, next => {
      next.state = 'starting';
      next.resources.launchedProcess = null;
      next.resources.listenerProcesses = [];
      return next;
    });
  let child: ChildProcess | null = null;
  try {
    const started = spawn(cli, ['start', '--listen-addr', `127.0.0.1:${port}`,
      '--data-dir', leaseDataDir(lease.resources.dataDir)], { detached: true, stdio: 'ignore', windowsHide: true });
    child = started;
    const spawnFailed = new Promise<never>((_, reject) => started.once('error', reject));
    started.unref();
    if (!started.pid) throw new Error('SpacetimeDB restart did not return a process id');
    updateBackendLease(leasePath,
      { token: lease.ownershipToken, backend: 'spacetime', runId: lease.runId }, next => {
        const identity = processIdentity(started.pid!);
        if (!identity) throw new Error('could not record SpacetimeDB restart process identity');
        next.resources.launchedProcess = identity;
        return next;
      });
    await Promise.race([
      waitFor(() => answers(`${lease.resources.serverUri}/v1/ping`),
        240_000, 'SpacetimeDB to start', signal),
      spawnFailed,
    ]);
    const listenerPids = pidsOnPort(port);
    if (listenerPids.length !== 1) throw new Error(`expected one SpacetimeDB listener, found ${listenerPids.length}`);
    const listenerIdentity = processIdentity(listenerPids[0]!);
    if (!listenerIdentity) throw new Error('could not record SpacetimeDB listener identity');
    updateBackendLease(leasePath,
      { token: lease.ownershipToken, backend: 'spacetime', runId: lease.runId }, next => {
        next.state = 'active';
        next.resources.listenerProcesses = [listenerIdentity];
        return next;
      });
  } catch (error) {
    if (child?.pid) killDetachedTree(child.pid);
    throw error;
  }
}

export function activateSpacetime({ leasePath, leaseToken, lease, cli }: {
  leasePath: string; leaseToken: string; lease: BackendLease; cli?: string;
}): void {
  const url = leaseUrl(lease.resources.serverUri);
  const port = Number(url.port);
  const ping = `${lease.resources.serverUri}/v1/ping`;
  if (answersSync(ping, 5)) {
    throw new Error(`SpacetimeDB is already running on benchmark port :${port}; `
      + 'refusing to reuse or restart a host this run did not start');
  }
  if (!cli || !existsSync(cli)) throw new Error(`SpacetimeDB CLI is unavailable: ${cli ?? '<unset>'}`);
  console.log(`  spacetime   ... not running, starting a benchmark-owned host on :${port}`);
  mkdirSync(leaseDataDir(lease.resources.dataDir), { recursive: true });
  updateBackendLease(leasePath,
    { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
      next.state = 'starting';
      return next;
    });
  let child: ChildProcess | null = null;
  try {
    const started = spawn(cli,
      ['start', '--listen-addr', `127.0.0.1:${port}`, '--data-dir', leaseDataDir(lease.resources.dataDir)],
      { detached: true, stdio: 'ignore', windowsHide: true });
    child = started;
    started.once('error', () => { /* surfaced by the missing-pid guard below */ });
    started.unref();
    if (!started.pid) throw new Error('SpacetimeDB start did not return a process id');
    updateBackendLease(leasePath,
      { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
        const identity = processIdentity(started.pid!);
        if (!identity) throw new Error('could not record SpacetimeDB start process identity');
        next.resources.launchedProcess = identity;
        return next;
      });
    for (let i = 0; i < 60; i++) {
      sleepSync(2000);
      if (answersSync(ping, 5)) {
        const listenerPids = pidsOnPort(port);
        if (listenerPids.length !== 1) {
          throw new Error(`Expected one SpacetimeDB listener on :${port}, found ${listenerPids.length}`);
        }
        const launchedIdentity = processIdentity(started.pid!);
        const listenerIdentity = processIdentity(listenerPids[0]!);
        if (!launchedIdentity || !listenerIdentity) {
          throw new Error('could not record SpacetimeDB process identity');
        }
        updateBackendLease(leasePath,
          { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
            next.resources.launchedProcess = launchedIdentity;
            next.resources.listenerProcesses = [listenerIdentity];
            next.state = 'active';
            return next;
          });
        console.log(`  spacetime   ... up (lease ${lease.runId}, listener PID ${listenerPids[0]})`);
        return;
      }
    }
    throw new Error(`SpacetimeDB did not come up on :${port}`);
  } catch (error) {
    if (child?.pid) killDetachedTree(child.pid);
    throw error;
  }
}

