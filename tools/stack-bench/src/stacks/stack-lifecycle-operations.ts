import { execFileSync, spawn } from 'node:child_process';
import type { ChildProcess } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { updateBackendLease } from '../runtime/backend-lease.js';
import { answers as answersSync, killDetachedTree, killTree, pidsOnPort, processIdentity,
  processIdentityMatches, sleepSync } from '../runtime/platform.js';
import { fetchStatus } from '../runtime/readiness.js';
import { redactCredentials } from '../evidence/diagnostic-sanitizer.js';
import { CODING_CONTAINER_AGENT, CODING_CONTAINER_CONTROL_DIR, codingContainerAgentExecOptions }
  from '../runtime/coding-container-policy.js';
import type { BackendLease, BackendLeaseContainer } from '../runtime/backend-lease.js';
import type { TextCommandExecutor } from '../runtime/command-executor.js';

export interface ApplicationControlInput {
  adapterId?: unknown;
  lease: { resources: { buildContainer?: unknown } };
  app?: unknown;
  port?: unknown;
  probe?: unknown;
  mode?: unknown;
  environment?: Record<string, string>;
  signal?: unknown;
  exec?: TextCommandExecutor;
}

const isOwnedContainer = (value: unknown): value is BackendLeaseContainer =>
  value !== null && typeof value === 'object' && 'owned' in value && Boolean(value.owned);

// A SpacetimeDB lease always records the host it claimed and where it keeps
// its data.
const leaseDataDir = (dataDir: string | null | undefined): string => {
  if (!dataDir) throw new Error('SpacetimeDB lease records no data directory');
  return dataDir;
};

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
  { freshConnection = false, requireSuccess = false }: {
    freshConnection?: boolean; requireSuccess?: boolean;
  } = {}): Promise<boolean> {
  const status = await fetchStatus(url, { timeoutMs: 5000,
    ...(freshConnection ? { init: { headers: { connection: 'close' } } } : {}) });
  return status !== null && (!requireSuccess || (status >= 200 && status < 300));
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
  exec: TextCommandExecutor = execFileSync): BackendLeaseContainer {
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
    throw new Error(`invalid hosted application port ${port}`);
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
    + `echo "hosted application port ${numericPort} still has a listener" >&2; exit 4`;
}

export function hostedRecordedProcessStopScript(record: string): string {
  if (!record.startsWith(`${CONTROL_DIR}/restart-`) || !record.endsWith('.pid')
    || /[\s'"`;\\]/.test(record)) {
    throw new Error('invalid hosted application process record');
  }
  return `record=${record}; [ -f "$record" ] || exit 0; `
    + 'set -- $(cat "$record"); [ "$#" -eq 2 ] || { echo "invalid application process record" >&2; exit 4; }; '
    + 'pid="$1"; started="$2"; case "$pid:$started" in *[!0-9:]*) '
    + 'echo "unsafe application process identity" >&2; exit 4;; esac; '
    + '[ "$pid" -gt 1 ] && [ "$started" -gt 0 ] '
    + '|| { echo "unsafe application process identity" >&2; exit 4; }; '
    + 'self_pgid=$(ps -o pgid= -p $$ | tr -d " "); '
    + '[ "$pid" != "$self_pgid" ] || { echo "application process group matches controller" >&2; exit 4; }; '
    + 'if [ -r "/proc/$pid/stat" ]; then stat=$(cat "/proc/$pid/stat"); rest=${stat##*) }; '
    + 'set -- $rest; current_pgid="$3"; current_started="${20}"; '
    + 'if [ "$current_pgid" != "$pid" ] || [ "$current_started" != "$started" ]; then '
    + 'rm -f "$record"; exit 0; fi; fi; '
    + 'if /bin/kill -0 -- "-$pid" 2>/dev/null; then '
    + '/bin/kill -TERM -- "-$pid" 2>/dev/null || true; attempt=0; '
    + 'while /bin/kill -0 -- "-$pid" 2>/dev/null && [ "$attempt" -lt 100 ]; do '
    + 'attempt=$((attempt + 1)); sleep 0.1; done; '
    + 'if /bin/kill -0 -- "-$pid" 2>/dev/null; then /bin/kill -KILL -- "-$pid" 2>/dev/null || true; fi; '
    + 'attempt=0; while /bin/kill -0 -- "-$pid" 2>/dev/null && [ "$attempt" -lt 50 ]; do '
    + 'attempt=$((attempt + 1)); sleep 0.1; done; '
    + '/bin/kill -0 -- "-$pid" 2>/dev/null '
    + '&& { echo "application process group is still running" >&2; exit 4; }; fi; '
    + 'rm -f "$record"';
}

export function hostedLaunchCommand(app: string): { directory: string; command: string } {
  if (existsSync(join(app, 'start.sh'))) {
    return { directory: '.', command: '/bin/bash ./start.sh' };
  }
  const packagePath = join(app, 'package.json');
  let packageJson: unknown = {};
  if (existsSync(packagePath)) {
    try {
      packageJson = JSON.parse(readFileSync(packagePath, 'utf8'));
    } catch (cause) {
      throw Object.assign(new Error('package.json is not valid JSON', { cause }),
        { code: 'generated_app_not_restartable' });
    }
  }
  if (packageJson !== null && typeof packageJson === 'object'
    && 'scripts' in packageJson && packageJson.scripts !== null
    && typeof packageJson.scripts === 'object' && 'start' in packageJson.scripts
    && typeof packageJson.scripts.start === 'string' && packageJson.scripts.start.trim()) {
    return { directory: '.', command: '/usr/local/bin/npm run start' };
  }
  throw Object.assign(new Error('app has no start.sh or npm start script'),
    { code: 'generated_app_not_restartable' });
}

export function captureHostedDiagnostics({ lease, output, exec = execFileSync }: {
  lease: BackendLease; output: string; exec?: TextCommandExecutor;
}): { captured: boolean; reason?: string; path?: string } {
  const container = inspectBuildContainer(lease, exec);
  const contents = exec('docker', ['exec', container.name, 'sh', '-c',
    `for f in ${CONTROL_DIR}/reference-application.log `
      + `${CONTROL_DIR}/restart-*.log; do `
      + '[ -f "$f" ] || continue; printf "===== %s =====\\n" "$f"; tail -n 400 "$f"; done'],
  { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
  if (!contents.trim()) return { captured: false, reason: 'no restart log was present' };
  writeFileSync(output, contents);
  return { captured: true, path: output };
}

export async function controlApplication({ adapterId: stack, lease, app, port, probe, mode,
  environment = {}, signal, exec = execFileSync }: ApplicationControlInput): Promise<void> {
  const abort = signal instanceof AbortSignal ? signal : null;
  if (typeof stack !== 'string' || !/^[a-z][a-z0-9-]*$/.test(stack)) {
    throw new Error('application control requires a valid stack id');
  }
  if (!Number.isInteger(Number(port)) || Number(port) <= 0 || Number(port) > 65535
    || typeof probe !== 'string') {
    throw new Error('application control requires a port and probe');
  }
  const container = inspectBuildContainer(lease, exec);
  const url = `http://127.0.0.1:${port}${probe}`;
  const processRecord = `${CONTROL_DIR}/restart-${stack}-${Number(port)}.pid`;
  if (mode !== 'start') {
    exec('docker', ['exec', container.name, 'sh', '-c',
      hostedRecordedProcessStopScript(processRecord)],
    { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
    // The reduced root capability set cannot inspect the agent user's sockets.
    // Stop the service as its owner so listener discovery and signals are reliable.
    exec('docker', ['exec', ...codingContainerAgentExecOptions(), container.name,
      'sh', '-c', hostedStopScript(Number(port))],
      { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
    await waitFor(async () => !(await answers(url, { freshConnection: true })),
      30_000, `${stack} application to stop`, abort);
  }
  if (mode === 'stop') return;
  if (typeof app !== 'string') throw new Error('application control requires an app directory');
  exec('docker', ['exec', ...codingContainerAgentExecOptions(), container.name, 'sh', '-c',
    `pids=$(lsof -ti tcp:${Number(port)} -sTCP:LISTEN | sort -u); `
      + '[ -z "$pids" ] || { echo "hosted application port is still owned by $pids" >&2; exit 4; }'],
  { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
  const launch = hostedLaunchCommand(app);
  const environmentArgs = Object.entries(environment).flatMap(([key, value]) => {
    if (!/^[A-Z][A-Z0-9_]*$/.test(key) || typeof value !== 'string' || /[\r\n\0]/.test(value)) {
      throw new Error(`invalid hosted runtime environment entry ${key}`);
    }
    return ['-e', `${key}=${value}`];
  });
  const log = `${CONTROL_DIR}/restart-${stack}-${Number(port)}.log`;
  exec('docker', ['exec', '-d', '-w', launch.directory === '.' ? '/app' : `/app/${launch.directory}`,
    '-e', `HOME=${CODING_CONTAINER_AGENT.home}`, '-e', `USER=${CODING_CONTAINER_AGENT.name}`,
    '-e', `PORT=${Number(port)}`, ...environmentArgs, container.name, 'sh', '-c',
    `set -eu; umask 000; : > ${log}; rm -f ${processRecord}; `
      + `exec /usr/bin/setsid sh -c 'stat=$(cat /proc/$$/stat); rest=${'${stat##*) }'}; `
      + `set -- $rest; (umask 077; printf "%s %s\\n" "$$" "${'${20}'}" > ${processRecord}); `
      + `exec /usr/bin/setpriv --reuid=${APP_UID} --regid=${APP_GID} --init-groups `
      + `${launch.command}' > ${log} 2>&1`],
  { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
  exec('docker', ['exec', container.name, 'sh', '-c',
    `attempt=0; while [ ! -s ${processRecord} ] && [ "$attempt" -lt 100 ]; do `
      + 'attempt=$((attempt + 1)); sleep 0.05; done; '
      + `[ -s ${processRecord} ] || { echo "application process record was not created" >&2; exit 4; }`],
  { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
  try {
    await waitFor(() => answers(url, { requireSuccess: true }),
      180_000, `${stack} application to start`, abort);
  } catch (cause) {
    let detail = '';
    try {
      detail = redactCredentials(exec('docker', ['exec', container.name, 'tail', '-n', '40', log],
        { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS }))
        .trim().split(/\r?\n/).slice(-3).join(' | ').slice(0, 500);
    } catch { /* the startup error remains useful without a log */ }
    throw Object.assign(new Error(`${stack} application did not start${detail ? `: ${detail}` : ''}`, { cause }),
      { code: 'generated_app_not_restartable' });
  }
  exec('docker', ['exec', ...codingContainerAgentExecOptions(), container.name, 'sh', '-c',
    `pids=$(lsof -ti tcp:${Number(port)} -sTCP:LISTEN | sort -u); `
      + 'set -- $pids; [ "$#" -eq 1 ] || { echo "expected one application listener, found: $pids" >&2; exit 4; }; '
      + 'pgid=$(ps -o pgid= -p "$1" | tr -d " "); '
      + 'case "$pgid" in ""|*[!0-9]*|1) echo "unsafe application process group" >&2; exit 4;; esac'],
  { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
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
