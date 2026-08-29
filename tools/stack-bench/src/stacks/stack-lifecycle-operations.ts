import { execFileSync, spawn } from 'node:child_process';
import type { ChildProcess } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { updateBackendLease } from '../runtime/backend-lease.js';
import { answers as answersSync, killDetachedTree, killTree, pidsOnPort, sleepSync } from '../runtime/platform.js';
import { fetchStatus } from '../runtime/readiness.js';
import { CODING_CONTAINER_AGENT, CODING_CONTAINER_CONTROL_DIR, codingContainerAgentExecOptions }
  from '../runtime/coding-container-policy.js';
import type { BackendLease, BackendLeaseContainer } from '../runtime/backend-lease.js';

// Control input arrives through the capability dispatch, so this function
// proves the port, probe, and build container itself.
export interface HostedControlInput {
  adapterId?: unknown;
  lease: { resources: { buildContainer?: unknown } };
  app?: unknown;
  port?: unknown;
  probe?: unknown;
  mode?: unknown;
  environment?: Record<string, string>;
  signal?: unknown;
  exec?: Exec;
}

type Exec = typeof execFileSync;

const isOwnedContainer = (value: unknown): value is BackendLeaseContainer =>
  value !== null && typeof value === 'object' && 'owned' in value && Boolean(value.owned);

// A SpacetimeDB lease always records the host it claimed.
const leaseUrl = (serverUri: string | null): URL => {
  if (!serverUri) throw new Error('SpacetimeDB lease records no server URI');
  return new URL(serverUri);
};

const DOCKER_TIMEOUT_MS = 120_000;
const CONTROL_DIR = CODING_CONTAINER_CONTROL_DIR;
const { uid: APP_UID, gid: APP_GID } = CODING_CONTAINER_AGENT;

const delay = (ms: number, signal?: AbortSignal | null): Promise<void> =>
  new Promise<void>((resolve, reject) => {
    if (signal?.aborted) { reject(signal.reason ?? new Error('backend control cancelled')); return; }
    const timer = setTimeout(done, ms);
    function done(): void {
      signal?.removeEventListener('abort', cancelled);
      resolve();
    }
    function cancelled(): void {
      clearTimeout(timer);
      signal?.removeEventListener('abort', cancelled);
      reject(signal?.reason ?? new Error('backend control cancelled'));
    }
    signal?.addEventListener('abort', cancelled, { once: true });
  });

async function answers(url: string,
  { freshConnection = false }: { freshConnection?: boolean } = {}): Promise<boolean> {
  const status = await fetchStatus(url, { timeoutMs: 5000,
    ...(freshConnection ? { init: { headers: { connection: 'close' } } } : {}) });
  return status !== null && status >= 200 && status < 300;
}

async function waitFor(check: () => Promise<boolean>, timeoutMs: number,
  description: string, signal?: AbortSignal | null): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (signal?.aborted) throw signal.reason ?? new Error('backend control cancelled');
    if (await check()) return;
    await delay(500, signal);
  }
  throw new Error(`timed out waiting for ${description}`);
}

function inspectBuildContainer(lease: { resources: { buildContainer?: unknown } },
  exec: Exec = execFileSync): BackendLeaseContainer {
  const container = lease.resources.buildContainer;
  if (!isOwnedContainer(container)) throw new Error('lease has no owned build container');
  const actual = exec('docker', ['inspect', '--format', '{{.Id}}', container.name],
    { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS }).trim();
  if (actual !== container.id) throw new Error(`${container.name} changed after lease creation; refusing control`);
  return container;
}

export function hostedStopScript(port: number | string): string {
  const numericPort = Number(port);
  if (!Number.isInteger(numericPort) || numericPort <= 0 || numericPort > 65535) {
    throw new Error(`invalid hosted backend port ${port}`);
  }
  const listeners = `lsof -ti tcp:${numericPort} -sTCP:LISTEN`;
  const protectedGroups = 'self_pgid=$(ps -o pgid= -p $$ | tr -d " "); '
    + 'init_child=$(ps -o pid= --ppid 1 | awk "NR==1 {print \\$1}"); '
    + 'init_pgid=""; [ -z "$init_child" ] || init_pgid=$(ps -o pgid= -p "$init_child" | tr -d " ")';
  const collectTargets = `groups=""; direct=""; for pid in $(${listeners}); do `
    + 'case "$pid" in ""|*[!0-9]*|1) echo "unsafe listener pid $pid" >&2; exit 4;; esac; '
    + 'pgid=$(ps -o pgid= -p "$pid" | tr -d " "); '
    + 'case "$pgid" in "") continue;; '
    + '*[!0-9]*|0) echo "unsafe process group for listener $pid" >&2; exit 4;; '
    + '*) if [ "$pgid" = 1 ] || [ "$pgid" = "$self_pgid" ] || [ "$pgid" = "$init_pgid" ]; '
    + 'then direct="$direct $pid"; '
    + 'else case " $groups " in *" $pgid "*) ;; *) groups="$groups $pgid";; esac; fi;; esac; done';
  const signalTargets = (signal: string): string => `for pgid in $groups; do /bin/kill -${signal} -- "-$pgid" 2>/dev/null || true; done; `
    + `for pid in $direct; do /bin/kill -${signal} "$pid" 2>/dev/null || true; done`;
  return `${protectedGroups}; quiet=0; attempt=0; while [ "$attempt" -lt 100 ]; do ${collectTargets}; `
    + 'if [ -z "$groups$direct" ]; then quiet=$((quiet + 1)); [ "$quiet" -ge 10 ] && exit 0; '
    + `else quiet=0; ${signalTargets('TERM')}; fi; `
    + 'attempt=$((attempt + 1)); sleep 0.1; done; '
    + `${collectTargets}; ${signalTargets('KILL')}; `
    + `quiet=0; attempt=0; while [ "$attempt" -lt 50 ]; do if [ -z "$(${listeners})" ]; `
    + 'then quiet=$((quiet + 1)); [ "$quiet" -ge 10 ] && exit 0; else quiet=0; fi; '
    + 'attempt=$((attempt + 1)); sleep 0.1; done; '
    + `echo "hosted backend port ${numericPort} still has a listener" >&2; exit 4`;
}

export function captureHostedDiagnostics({ lease, output, exec = execFileSync }: {
  lease: BackendLease; output: string; exec?: Exec;
}): { captured: boolean; reason?: string; path?: string } {
  const container = inspectBuildContainer(lease, exec);
  const contents = exec('docker', ['exec', container.name, 'sh', '-c',
    `for f in ${CONTROL_DIR}/reference-server.log ${CONTROL_DIR}/reference-client.log `
      + `${CONTROL_DIR}/restart-*.log; do `
      + '[ -f "$f" ] || continue; printf "===== %s =====\\n" "$f"; tail -n 400 "$f"; done'],
  { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
  if (!contents.trim()) return { captured: false, reason: 'no restart log was present' };
  writeFileSync(output, contents);
  return { captured: true, path: output };
}

export async function controlHosted({ adapterId: backend, lease, app, port, probe, mode,
  environment = {}, signal, exec = execFileSync }: HostedControlInput): Promise<void> {
  const abort = signal instanceof AbortSignal ? signal : null;
  if (!Number.isInteger(Number(port)) || Number(port) <= 0 || typeof probe !== 'string'
    || typeof app !== 'string') {
    throw new Error('hosted backend control requires a port and probe');
  }
  const container = inspectBuildContainer(lease, exec);
  const url = `http://127.0.0.1:${port}${probe}`;
  if (mode !== 'start') {
    // The reduced root capability set cannot inspect the agent user's sockets.
    // Stop the service as its owner so listener discovery and signals are reliable.
    exec('docker', ['exec', ...codingContainerAgentExecOptions(), container.name,
      'sh', '-c', hostedStopScript(Number(port))],
      { stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
    await waitFor(async () => !(await answers(url, { freshConnection: true })),
      30_000, `${backend} API to stop`, abort);
  }
  if (mode === 'stop') return;
  exec('docker', ['exec', ...codingContainerAgentExecOptions(), container.name, 'sh', '-c',
    `pids=$(lsof -ti tcp:${Number(port)} -sTCP:LISTEN | sort -u); `
      + '[ -z "$pids" ] || { echo "hosted backend port is still owned by $pids" >&2; exit 4; }'],
  { stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
  const serverRelative = existsSync(join(app, 'server', 'package.json')) ? 'server' : '.';
  const packageJson = JSON.parse(readFileSync(join(app, serverRelative, 'package.json'), 'utf8'));
  // Prefer a stable server process when the app provides one. A file watcher
  // can restart while an upgrade replaces source files and start two seeders
  // against the same freshly reset database.
  const script = packageJson.scripts?.start ? 'start' : packageJson.scripts?.dev ? 'dev' : null;
  if (!script) {
    throw Object.assign(new Error(`${serverRelative}/package.json has no dev or start script`),
      { code: 'generated_app_not_restartable' });
  }
  const environmentArgs = Object.entries(environment).flatMap(([key, value]) => {
    if (!/^[A-Z][A-Z0-9_]*$/.test(key) || typeof value !== 'string' || /[\r\n\0]/.test(value)) {
      throw new Error(`invalid hosted runtime environment entry ${key}`);
    }
    return ['-e', `${key}=${value}`];
  });
  const log = `${CONTROL_DIR}/restart-${backend}-${Number(port)}.log`;
  exec('docker', ['exec', '-d', '-w', serverRelative === '.' ? '/app' : `/app/${serverRelative}`,
    '-e', `HOME=${CODING_CONTAINER_AGENT.home}`, '-e', `USER=${CODING_CONTAINER_AGENT.name}`,
    '-e', `PORT=${Number(port)}`, ...environmentArgs, container.name, 'sh', '-c',
    `set -eu; umask 000; : > ${log}; `
      + `exec /usr/bin/setpriv --reuid=${APP_UID} --regid=${APP_GID} --init-groups `
      + `/usr/local/bin/npm run ${script} > ${log} 2>&1`],
  { stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
  await waitFor(() => answers(url), 180_000, `${backend} API to start`, abort);
  exec('docker', ['exec', ...codingContainerAgentExecOptions(), container.name, 'sh', '-c',
    `pids=$(lsof -ti tcp:${Number(port)} -sTCP:LISTEN | sort -u); `
      + 'set -- $pids; [ "$#" -eq 1 ] || { echo "expected one hosted backend listener, found: $pids" >&2; exit 4; }; '
      + 'pgid=$(ps -o pgid= -p "$1" | tr -d " "); '
      + 'case "$pgid" in ""|*[!0-9]*|1) echo "unsafe hosted backend process group" >&2; exit 4;; esac'],
  { stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
}

export async function controlSpacetime({ lease, mode, signal = null }: {
  lease: BackendLease; mode: string; signal?: AbortSignal | null;
}): Promise<void> {
  const leasePath = process.env.STACK_BENCH_LEASE ?? '';
  if (mode !== 'restart') return;
  const url = leaseUrl(lease.resources.serverUri);
  const port = Number(url.port);
  const cli = process.env.SPACETIME_BIN;
  if (!cli || !existsSync(cli)) throw new Error(`SPACETIME_BIN is unavailable: ${cli ?? '<unset>'}`);
  const actual = pidsOnPort(port);
  const unexpected = actual.filter(pid => !lease.resources.listenerPids.includes(pid));
  if (unexpected.length) throw new Error(`listener ${unexpected.join(',')} is not owned by lease ${lease.runId}`);
  updateBackendLease(leasePath,
    { token: lease.ownershipToken, backend: 'spacetime', runId: lease.runId }, next => {
      next.state = 'restarting'; return next;
    });
  for (const pid of actual) killTree(pid);
  await waitFor(async () => !(await answers(`${lease.resources.serverUri}/v1/ping`)),
    30_000, 'SpacetimeDB to stop', signal);
  updateBackendLease(leasePath,
    { token: lease.ownershipToken, backend: 'spacetime', runId: lease.runId }, next => {
      next.state = 'starting';
      next.resources.launchedPid = null;
      next.resources.listenerPids = [];
      return next;
    });
  let child: ChildProcess | null = null;
  try {
    const started = spawn(cli, ['start', '--listen-addr', `127.0.0.1:${port}`,
      '--data-dir', lease.resources.dataDir ?? ''], { detached: true, stdio: 'ignore', windowsHide: true });
    child = started;
    const spawnFailed = new Promise<never>((_, reject) => started.once('error', reject));
    started.unref();
    if (!started.pid) throw new Error('SpacetimeDB restart did not return a process id');
    updateBackendLease(leasePath,
      { token: lease.ownershipToken, backend: 'spacetime', runId: lease.runId }, next => {
        next.resources.launchedPid = started.pid ?? null;
        return next;
      });
    await Promise.race([
      waitFor(() => answers(`${lease.resources.serverUri}/v1/ping`),
        240_000, 'SpacetimeDB to start', signal),
      spawnFailed,
    ]);
    const listenerPids = pidsOnPort(port);
    if (listenerPids.length !== 1) throw new Error(`expected one SpacetimeDB listener, found ${listenerPids.length}`);
    updateBackendLease(leasePath,
      { token: lease.ownershipToken, backend: 'spacetime', runId: lease.runId }, next => {
        next.state = 'active';
        next.resources.listenerPids = listenerPids;
        return next;
      });
  } catch (error) {
    if (child?.pid) killDetachedTree(child.pid);
    throw error;
  }
}

export function activateHosted({ leasePath, leaseToken, lease }: {
  leasePath: string; leaseToken: string; lease: BackendLease;
}): void {
  updateBackendLease(leasePath,
    { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
      next.state = 'active';
      return next;
    });
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
  mkdirSync(lease.resources.dataDir ?? '', { recursive: true });
  updateBackendLease(leasePath,
    { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
      next.state = 'starting';
      return next;
    });
  let child: ChildProcess | null = null;
  try {
    const started = spawn(cli,
      ['start', '--listen-addr', `127.0.0.1:${port}`, '--data-dir', lease.resources.dataDir ?? ''],
      { detached: true, stdio: 'ignore', windowsHide: true });
    child = started;
    started.once('error', () => { /* surfaced by the missing-pid guard below */ });
    started.unref();
    if (!started.pid) throw new Error('SpacetimeDB start did not return a process id');
    updateBackendLease(leasePath,
      { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
        next.resources.launchedPid = started.pid ?? null;
        return next;
      });
    for (let i = 0; i < 60; i++) {
      sleepSync(2000);
      if (answersSync(ping, 5)) {
        const listenerPids = pidsOnPort(port);
        if (listenerPids.length !== 1) {
          throw new Error(`Expected one SpacetimeDB listener on :${port}, found ${listenerPids.length}`);
        }
        updateBackendLease(leasePath,
          { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
            next.resources.launchedPid = started.pid ?? null;
            next.resources.listenerPids = listenerPids;
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
