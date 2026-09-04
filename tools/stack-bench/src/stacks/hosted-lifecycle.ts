import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { updateBackendLease } from '../runtime/backend-lease.js';
import { redactCredentials } from '../evidence/diagnostic-sanitizer.js';
import { CODING_CONTAINER_AGENT, CODING_CONTAINER_APP_ROOT, CODING_CONTAINER_CONTROL_DIR,
  codingContainerAgentExecOptions, codingContainerWorkspaceHandoffCommands }
  from '../runtime/coding-container-policy.js';
import type { BackendLease, BackendLeaseContainer } from '../runtime/backend-lease.js';
import type { TextCommandExecutor } from '../runtime/command-executor.js';
import { answers, waitFor } from './lifecycle-readiness.js';
import type { RuntimeControlMode } from './stack-adapter-contract.js';

interface HostedApplicationControlInput {
  adapterId: string;
  lease: { resources: { buildContainer?: BackendLeaseContainer | null } };
  app: string;
  port: number;
  probe: string;
  mode: RuntimeControlMode;
  environment?: Record<string, string>;
  signal?: AbortSignal | null;
  handoffWorkspace?: boolean;
  exec?: TextCommandExecutor;
}

const isOwnedContainer = (value: unknown): value is BackendLeaseContainer =>
  value !== null && typeof value === 'object' && 'owned' in value && Boolean(value.owned);

const DOCKER_TIMEOUT_MS = 120_000;
// A start from clean source may install packages over the network before it
// listens, so the budget is generous. A launch that has exited without leaving
// a listener fails as soon as that is seen, not at the deadline.
export const HOSTED_START_TIMEOUT_MS = 900_000;
const START_LOG_LINES = 200;
const CONTROL_DIR = CODING_CONTAINER_CONTROL_DIR;
const { uid: APP_UID, gid: APP_GID } = CODING_CONTAINER_AGENT;

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
  const contents = exec('docker', ['exec', container.id, 'sh', '-c',
    `for f in ${CONTROL_DIR}/reference-application.log `
      + `${CONTROL_DIR}/restart-*.log; do `
      + '[ -f "$f" ] || continue; printf "===== %s =====\\n" "$f"; tail -n 400 "$f"; done'],
  { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
  if (!contents.trim()) return { captured: false, reason: 'no restart log was present' };
  writeFileSync(output, contents);
  return { captured: true, path: output };
}

export async function controlHostedAppServer({ adapterId: stack, lease, app, port, probe, mode,
  environment = {}, signal, handoffWorkspace = false,
  exec = execFileSync }: HostedApplicationControlInput): Promise<void> {
  if (mode !== 'start' && mode !== 'stop' && mode !== 'restart') {
    throw new Error(`unsupported application control mode ${String(mode)}`);
  }
  const abort = signal ?? null;
  if (!/^[a-z][a-z0-9-]*$/.test(stack)) {
    throw new Error('application control requires a valid stack id');
  }
  if (!Number.isInteger(port) || port <= 0 || port > 65535) {
    throw new Error('application control requires a port and probe');
  }
  const container = inspectBuildContainer(lease, exec);
  const url = `http://127.0.0.1:${port}${probe}`;
  const processRecord = `${CONTROL_DIR}/restart-${stack}-${Number(port)}.pid`;
  if (mode !== 'start') {
    exec('docker', ['exec', container.id, 'sh', '-c',
      hostedRecordedProcessStopScript(processRecord)],
    { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
    // The reduced root capability set cannot inspect the agent user's sockets.
    // Stop the service as its owner so listener discovery and signals are reliable.
    exec('docker', ['exec', ...codingContainerAgentExecOptions(), container.id,
      'sh', '-c', hostedStopScript(Number(port))],
      { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
    await waitFor(async () => !(await answers(url, { freshConnection: true })),
      30_000, `${stack} application to stop`, abort);
  }
  if (handoffWorkspace) {
    for (const command of codingContainerWorkspaceHandoffCommands(process.getgid?.() ?? 0)) {
      exec('docker', ['exec', container.id, ...command],
        { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
    }
  }
  if (mode === 'stop') return;
  if (typeof app !== 'string') throw new Error('application control requires an app directory');
  exec('docker', ['exec', ...codingContainerAgentExecOptions(), container.id, 'sh', '-c',
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
  exec('docker', ['exec', '-d', '-w', launch.directory === '.' ? CODING_CONTAINER_APP_ROOT
    : `${CODING_CONTAINER_APP_ROOT}/${launch.directory}`,
    '-e', `HOME=${CODING_CONTAINER_AGENT.home}`, '-e', `USER=${CODING_CONTAINER_AGENT.name}`,
    '-e', `PORT=${Number(port)}`, ...environmentArgs, container.id, 'sh', '-c',
    `set -eu; umask 022; : > ${log}; rm -f ${processRecord}; `
      + `exec /usr/bin/setsid sh -c 'stat=$(cat /proc/$$/stat); rest=${'${stat##*) }'}; `
      + `set -- $rest; (umask 077; printf "%s %s\\n" "$$" "${'${20}'}" > ${processRecord}); `
      + `exec /usr/bin/setpriv --reuid=${APP_UID} --regid=${APP_GID} --init-groups `
      + `${launch.command}' > ${log} 2>&1`],
  { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
  exec('docker', ['exec', container.id, 'sh', '-c',
    `attempt=0; while [ ! -s ${processRecord} ] && [ "$attempt" -lt 100 ]; do `
      + 'attempt=$((attempt + 1)); sleep 0.05; done; '
      + `[ -s ${processRecord} ] || { echo "application process record was not created" >&2; exit 4; }`],
  { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
  const commandSucceeds = (args: string[]): boolean => {
    try {
      exec('docker', args, { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
      return true;
    } catch { return false; }
  };
  const launchedProcessAlive = () => commandSucceeds(['exec', container.id, 'sh', '-c',
    `read pid rest < ${processRecord} && [ -d "/proc/$pid" ]`]);
  const portHasListener = () => commandSucceeds(['exec', ...codingContainerAgentExecOptions(),
    container.id, 'sh', '-c', `[ -n "$(lsof -ti tcp:${Number(port)} -sTCP:LISTEN)" ]`]);
  // The launch log is the only account of why an application did not start.
  // The error carries it whole; the message keeps the last lines.
  const startFailure = (what: string, cause: unknown): Error => {
    let startLog = '';
    try {
      startLog = redactCredentials(exec('docker',
        ['exec', container.id, 'tail', '-n', String(START_LOG_LINES), log],
        { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS })).trim();
    } catch { /* the startup error remains useful without a log */ }
    const detail = startLog.split(/\r?\n/).slice(-3).join(' | ').slice(0, 500);
    return Object.assign(new Error(`${stack} application ${what}${detail ? `: ${detail}` : ''}`, { cause }),
      { code: 'generated_app_not_restartable', startLog });
  };
  let exited = false;
  try {
    await waitFor(async () => {
      if (await answers(url, { requireSuccess: true })) return true;
      if (!launchedProcessAlive() && !portHasListener()) {
        exited = true;
        throw new Error(`${stack} application process exited`);
      }
      return false;
    }, HOSTED_START_TIMEOUT_MS, `${stack} application to start`, abort);
  } catch (cause) {
    throw startFailure(exited ? `exited before it listened on port ${Number(port)}` : 'did not start', cause);
  }
  exec('docker', ['exec', ...codingContainerAgentExecOptions(), container.id, 'sh', '-c',
    `pids=$(lsof -ti tcp:${Number(port)} -sTCP:LISTEN | sort -u); `
      + 'set -- $pids; [ "$#" -eq 1 ] || { echo "expected one application listener, found: $pids" >&2; exit 4; }; '
      + 'pgid=$(ps -o pgid= -p "$1" | tr -d " "); '
      + 'case "$pgid" in ""|*[!0-9]*|1) echo "unsafe application process group" >&2; exit 4;; esac'],
  { encoding: 'utf8', stdio: 'pipe', timeout: DOCKER_TIMEOUT_MS });
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
