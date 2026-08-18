import { execFileSync, spawn } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { updateBackendLease } from './backend-lease.mjs';
import { answers as answersSync, killDetachedTree, killTree, pidsOnPort, sleepSync } from './platform.mjs';
import { fetchStatus } from './readiness.mjs';

const DOCKER_TIMEOUT_MS = 120_000;

const delay = (ms, signal) => new Promise((resolve, reject) => {
  if (signal?.aborted) { reject(signal.reason ?? new Error('backend control cancelled')); return; }
  const timer = setTimeout(done, ms);
  function done() {
    signal?.removeEventListener('abort', cancelled);
    resolve();
  }
  function cancelled() {
    clearTimeout(timer);
    signal?.removeEventListener('abort', cancelled);
    reject(signal.reason ?? new Error('backend control cancelled'));
  }
  signal?.addEventListener('abort', cancelled, { once: true });
});

async function answers(url) {
  const status = await fetchStatus(url, { timeoutMs: 5000 });
  return status !== null && status >= 200 && status < 300;
}

async function waitFor(check, timeoutMs, description, signal) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (signal?.aborted) throw signal.reason ?? new Error('backend control cancelled');
    if (await check()) return;
    await delay(500, signal);
  }
  throw new Error(`timed out waiting for ${description}`);
}

function inspectBuildContainer(lease, exec = execFileSync) {
  const container = lease.resources.buildContainer;
  if (!container?.owned) throw new Error('lease has no owned build container');
  const actual = exec('docker', ['inspect', '--format', '{{.Id}}', container.name],
    { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS }).trim();
  if (actual !== container.id) throw new Error(`${container.name} changed after lease creation; refusing control`);
  return container;
}

export function hostedStopScript(port) {
  const numericPort = Number(port);
  if (!Number.isInteger(numericPort) || numericPort <= 0 || numericPort > 65535) {
    throw new Error(`invalid hosted backend port ${port}`);
  }
  return `pids=$(lsof -ti tcp:${numericPort} -sTCP:LISTEN); `
    + '[ -z "$pids" ] && exit 0; groups=""; '
    + 'for pid in $pids; do pgid=$(ps -o pgid= -p "$pid" | tr -d " "); '
    + 'case "$pgid" in ""|*[!0-9]*|1) echo "unsafe process group for listener $pid" >&2; exit 4;; esac; '
    + 'case " $groups " in *" $pgid "*) ;; *) groups="$groups $pgid";; esac; done; '
    + 'for pgid in $groups; do /bin/kill -TERM -- "-$pgid" 2>/dev/null || true; done; '
    + 'attempt=0; while [ "$attempt" -lt 50 ]; do alive=""; '
    + 'for pgid in $groups; do ps -eo pgid= | awk -v g="$pgid" \'$1 == g { found=1 } END { exit !found }\' '
    + '&& alive="$alive $pgid" || true; done; [ -z "$alive" ] && exit 0; '
    + 'groups="$alive"; attempt=$((attempt + 1)); sleep 0.1; done; '
    + 'for pgid in $groups; do /bin/kill -KILL -- "-$pgid" 2>/dev/null || true; done';
}

export function captureHostedDiagnostics({ lease, output, exec = execFileSync }) {
  const container = inspectBuildContainer(lease, exec);
  const contents = exec('docker', ['exec', container.name, 'sh', '-lc',
    'for f in /tmp/reference-server.log /tmp/reference-client.log /tmp/restart-*.log; do '
      + '[ -f "$f" ] || continue; printf "===== %s =====\\n" "$f"; tail -n 400 "$f"; done'],
  { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
  if (!contents.trim()) return { captured: false, reason: 'no restart log was present' };
  writeFileSync(output, contents);
  return { captured: true, path: output };
}

export async function controlHosted({ adapterId: backend, lease, app, port, probe, mode,
  signal = null, exec = execFileSync }) {
  if (!Number.isInteger(Number(port)) || Number(port) <= 0 || typeof probe !== 'string') {
    throw new Error('hosted backend control requires a port and probe');
  }
  const container = inspectBuildContainer(lease, exec);
  const url = `http://127.0.0.1:${port}${probe}`;
  if (mode !== 'start') {
    exec('docker', ['exec', container.name, 'sh', '-lc', hostedStopScript(port)],
      { stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
    await waitFor(async () => !(await answers(url)), 30_000, `${backend} API to stop`, signal);
  }
  if (mode === 'stop') return;
  exec('docker', ['exec', container.name, 'sh', '-lc',
    `pids=$(lsof -ti tcp:${Number(port)} -sTCP:LISTEN | sort -u); `
      + '[ -z "$pids" ] || { echo "hosted backend port is still owned by $pids" >&2; exit 4; }'],
  { stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
  const serverRelative = existsSync(join(app, 'server', 'package.json')) ? 'server' : '.';
  const packageJson = JSON.parse(readFileSync(join(app, serverRelative, 'package.json'), 'utf8'));
  const script = packageJson.scripts?.dev ? 'dev' : packageJson.scripts?.start ? 'start' : null;
  if (!script) {
    const error = new Error(`${serverRelative}/package.json has no dev or start script`);
    error.code = 'generated_app_not_restartable';
    throw error;
  }
  exec('docker', ['exec', '-d', '-w', serverRelative === '.' ? '/app' : `/app/${serverRelative}`,
    '-e', `PORT=${Number(port)}`, container.name, 'sh', '-lc',
    `exec npm run ${script} > /tmp/restart-${backend}-${Number(port)}.log 2>&1`],
  { stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
  await waitFor(() => answers(url), 180_000, `${backend} API to start`, signal);
  exec('docker', ['exec', container.name, 'sh', '-lc',
    `pids=$(lsof -ti tcp:${Number(port)} -sTCP:LISTEN | sort -u); `
      + 'set -- $pids; [ "$#" -eq 1 ] || { echo "expected one hosted backend listener, found: $pids" >&2; exit 4; }; '
      + 'pgid=$(ps -o pgid= -p "$1" | tr -d " "); '
      + 'case "$pgid" in ""|*[!0-9]*|1) echo "unsafe hosted backend process group" >&2; exit 4;; esac'],
  { stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
}

export async function controlSpacetime({ lease, mode, signal = null }) {
  if (mode !== 'restart') return;
  const url = new URL(lease.resources.serverUri);
  const port = Number(url.port);
  const cli = process.env.SPACETIME_BIN;
  if (!cli || !existsSync(cli)) throw new Error(`SPACETIME_BIN is unavailable: ${cli ?? '<unset>'}`);
  const actual = pidsOnPort(port);
  const unexpected = actual.filter(pid => !lease.resources.listenerPids.includes(pid));
  if (unexpected.length) throw new Error(`listener ${unexpected.join(',')} is not owned by lease ${lease.runId}`);
  updateBackendLease(process.env.STACK_BENCH_LEASE,
    { token: lease.ownershipToken, backend: 'spacetime', runId: lease.runId }, next => {
      next.state = 'restarting'; return next;
    });
  for (const pid of actual) killTree(pid);
  await waitFor(async () => !(await answers(`${lease.resources.serverUri}/v1/ping`)),
    30_000, 'SpacetimeDB to stop', signal);
  updateBackendLease(process.env.STACK_BENCH_LEASE,
    { token: lease.ownershipToken, backend: 'spacetime', runId: lease.runId }, next => {
      next.state = 'starting';
      next.resources.launchedPid = null;
      next.resources.listenerPids = [];
      return next;
    });
  let child = null;
  try {
    child = spawn(cli, ['start', '--listen-addr', `127.0.0.1:${port}`,
      '--data-dir', lease.resources.dataDir], { detached: true, stdio: 'ignore', windowsHide: true });
    const spawnFailed = new Promise((_, reject) => child.once('error', reject));
    child.unref();
    if (!child.pid) throw new Error('SpacetimeDB restart did not return a process id');
    updateBackendLease(process.env.STACK_BENCH_LEASE,
      { token: lease.ownershipToken, backend: 'spacetime', runId: lease.runId }, next => {
        next.resources.launchedPid = child.pid;
        return next;
      });
    await Promise.race([
      waitFor(() => answers(`${lease.resources.serverUri}/v1/ping`),
        240_000, 'SpacetimeDB to start', signal),
      spawnFailed,
    ]);
    const listenerPids = pidsOnPort(port);
    if (listenerPids.length !== 1) throw new Error(`expected one SpacetimeDB listener, found ${listenerPids.length}`);
    updateBackendLease(process.env.STACK_BENCH_LEASE,
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

export function activateHosted({ leasePath, leaseToken, lease }) {
  updateBackendLease(leasePath,
    { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
      next.state = 'active';
      return next;
    });
}

export function activateSpacetime({ leasePath, leaseToken, lease, cli }) {
  const url = new URL(lease.resources.serverUri);
  const port = Number(url.port);
  const ping = `${lease.resources.serverUri}/v1/ping`;
  if (answersSync(ping, 5)) {
    throw new Error(`SpacetimeDB is already running on benchmark port :${port}; `
      + 'refusing to reuse or restart a host this run did not start');
  }
  if (!cli || !existsSync(cli)) throw new Error(`SpacetimeDB CLI is unavailable: ${cli ?? '<unset>'}`);
  console.log(`  spacetime   ... not running, starting a benchmark-owned host on :${port}`);
  mkdirSync(lease.resources.dataDir, { recursive: true });
  updateBackendLease(leasePath,
    { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
      next.state = 'starting';
      return next;
    });
  let child = null;
  try {
    child = spawn(cli,
      ['start', '--listen-addr', `127.0.0.1:${port}`, '--data-dir', lease.resources.dataDir],
      { detached: true, stdio: 'ignore', windowsHide: true });
    child.once('error', () => { /* surfaced by the missing-pid guard below */ });
    child.unref();
    if (!child.pid) throw new Error('SpacetimeDB start did not return a process id');
    updateBackendLease(leasePath,
      { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
        next.resources.launchedPid = child.pid;
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
            next.resources.launchedPid = child.pid ?? null;
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
