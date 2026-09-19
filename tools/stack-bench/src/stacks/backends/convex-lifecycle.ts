import { createHash } from 'node:crypto';
import { backendResourceLockKeys, readBackendLease, updateBackendLease, verifyResourceLocks }
  from '../../runtime/backend-lease.js';
import type { BackendLease } from '../../runtime/backend-lease.js';
import { releaseBackendLease } from '../../runtime/backend-teardown.js';
import { CODING_CONTAINER_CONTROL_DIR } from '../../runtime/coding-container-policy.js';
import { attemptDocker, createAttemptNetwork, createAttemptContainer, installAttemptFirewall,
  createAttemptBrowser, requireAttemptNetwork } from '../../runtime/docker-network.js';
import { sleepSync } from '../../runtime/platform.js';
import type { StackRunPorts } from '../stack-adapter-contract.js';
import { stopHostedHost } from '../stack-teardown-operations.js';
import { hostedRecordedProcessStopScript } from '../hosted-lifecycle.js';
import { answers, waitFor } from '../lifecycle-readiness.js';
import { leaseFromEnv } from '../../runtime/backend-lease.js';
import { loadTrack, portsFor } from '../../composition/tracks.js';
import type { StackLifecycleInput } from '../stack-adapter-contract.js';

export const CONVEX_BACKEND_IMAGE = 'ghcr.io/get-convex/convex-backend@sha256:afbf4292df387c8f031a68d00048551cf1640ddf0013c51ac704a89d7e73e743';
export const CONVEX_PROCESS_RECORD = `${CODING_CONTAINER_CONTROL_DIR}/restart-convex.pid`;

interface ConvexLifecycleInput {
  leasePath: string; leaseToken: string; ports: StackRunPorts;
}

function claimedPorts(lease: BackendLease, ports: StackRunPorts): number[] {
  if (ports.express === null || ports.dbPort !== null) throw new Error('Convex requires frontend, native API and HTTP-action ports');
  const nativePort = Number(new URL(lease.resources.serverUri!).port);
  const endpoints = [ports.vite, ports.express, nativePort];
  if (new Set(endpoints).size !== endpoints.length) throw new Error('Convex endpoint ports must differ');
  const required = backendResourceLockKeys(lease, ports);
  if (required.some(key => !lease.resources.locks.some(lock => lock.key === key && !lock.releasedAt))) {
    throw new Error('Convex endpoint resources must be claimed before activation');
  }
  verifyResourceLocks(lease);
  return endpoints;
}

function startConvexProcess(lease: BackendLease, sitePort: number, timeoutMs = 60_000): void {
  const container = lease.resources.container!;
  const nativePort = Number(new URL(lease.resources.serverUri!).port);
  const instance = `sb-${createHash('sha256').update(lease.runId).digest('hex').slice(0, 20)}`;
  // The pinned vendor helper generates and stores the secret in this container's
  // anonymous /convex/data volume. It never enters command arguments or evidence.
  attemptDocker(['exec', '-d', container.id, 'bash', '-ec',
    `umask 077; mkdir -p ${CODING_CONTAINER_CONTROL_DIR} /convex/data/tmp /convex/data/storage; `
    + `export INSTANCE_NAME=${instance}; source ./read_credentials.sh; export TMPDIR=/convex/data/tmp; `
    + `exec setsid bash -ec 'stat=$(cat /proc/$$/stat); rest=\${stat##*) }; set -- $rest; `
    + `printf "%s %s\\n" "$$" "\${20}" > ${CONVEX_PROCESS_RECORD}; `
    + 'exec ./convex-local-backend --instance-name "$INSTANCE_NAME" --instance-secret "$INSTANCE_SECRET" '
    + `--port ${nativePort} --site-proxy-port ${sitePort} `
    + `--convex-origin http://127.0.0.1:${nativePort} --convex-site http://127.0.0.1:${sitePort} `
    + '--local-storage /convex/data/storage --disable-beacon --do-not-require-ssl /convex/data/db.sqlite3'
    + `' > ${CODING_CONTAINER_CONTROL_DIR}/stack-bench-backend.log 2>&1`]);
  const deadline = Date.now() + timeoutMs;
  while (true) {
    try {
      attemptDocker(['exec', container.id, 'curl', '--fail', '--silent', '--max-time', '2',
        `http://127.0.0.1:${nativePort}/version`]);
      break;
    } catch (error) {
      if (Date.now() >= deadline) throw new Error('Owned Convex backend did not become ready', { cause: error });
      sleepSync(250);
    }
  }
}

export function activateConvex({ leasePath, leaseToken, ports }: ConvexLifecycleInput): void {
  if (process.platform !== 'linux') throw new Error('Convex activation requires the Linux Docker controller');
  const lease = readBackendLease(leasePath, { token: leaseToken, backend: 'convex' });
  if (lease.state !== 'created' || lease.resources.container || lease.resources.creationIntents) {
    throw new Error('Convex activation requires a fresh lease');
  }
  const endpoints = claimedPorts(lease, ports);
  updateBackendLease(leasePath, { token: leaseToken }, next => { next.state = 'starting'; return next; });
  const image = attemptDocker(['image', 'inspect', '--format', '{{.Id}}', CONVEX_BACKEND_IMAGE]);
  let current = createAttemptNetwork(leasePath, lease);
  createAttemptContainer(leasePath, current, 'backend', image, current.resources.network!.id,
    endpoints.flatMap(port => ['--publish', `127.0.0.1:${port}:${port}`]));
  current = installAttemptFirewall(leasePath, readBackendLease(leasePath, { token: leaseToken }));
  startConvexProcess(current, ports.express!);
  createAttemptBrowser(leasePath, current);
  updateBackendLease(leasePath, { token: leaseToken }, next => { next.state = 'active'; return next; });
}

export function recoverConvex({ leasePath, leaseToken, ports, signal }: ConvexLifecycleInput & {
  signal?: AbortSignal;
}): void {
  signal?.throwIfAborted();
  const lease = readBackendLease(leasePath, { token: leaseToken, backend: 'convex', active: true });
  claimedPorts(lease, ports);
  requireAttemptNetwork(lease);
  const container = lease.resources.container!;
  const inspected = JSON.parse(attemptDocker(['inspect', container.name]))[0];
  if (inspected.Id !== container.id || inspected.Image !== container.image
    || !['', 'private'].includes(inspected.HostConfig.PidMode)) throw new Error('Convex container identity changed');
  // A crash must leave a valid record and no live process at that PID. Never
  // adopt a new process or erase state to make recovery succeed.
  attemptDocker(['exec', container.id, 'sh', '-ec',
    `read pid started < ${CONVEX_PROCESS_RECORD}; `
    + 'case "$pid:$started" in *[!0-9:]*) exit 4;; esac; '
    + '[ "$pid" -gt 1 ] && [ "$started" -gt 0 ] && [ ! -e /proc/$pid ]']);
  updateBackendLease(leasePath, { token: leaseToken }, next => { next.state = 'restarting'; return next; });
  startConvexProcess(lease, ports.express!, 30_000);
  updateBackendLease(leasePath, { token: leaseToken }, next => { next.state = 'active'; return next; });
  signal?.throwIfAborted();
}

export async function controlConvex({ leasePath, leaseToken, ports, mode }: ConvexLifecycleInput & {
  mode: 'restart' | 'reset';
}): Promise<void> {
  const lease = readBackendLease(leasePath, { token: leaseToken, backend: 'convex', active: true });
  claimedPorts(lease, ports);
  requireAttemptNetwork(lease);
  const container = lease.resources.container!;
  const inspected = JSON.parse(attemptDocker(['inspect', container.name]))[0];
  if (inspected.Id !== container.id || inspected.Image !== container.image
    || !['', 'private'].includes(inspected.HostConfig.PidMode)) throw new Error('Convex container identity changed');
  const dataMount = inspected.Mounts.find((mount: { Destination: string }) => mount.Destination === '/convex/data');
  if (mode === 'reset' && (dataMount?.Type !== 'volume' || dataMount.RW !== true
    || !/^[a-f0-9]{64}$/.test(dataMount.Name))) throw new Error('Convex reset requires its owned anonymous data volume');
  // The shared stop helper treats stale/missing records as already stopped.
  // Reset must instead refuse them before it can erase any state.
  const pid = attemptDocker(['exec', container.id, 'sh', '-ec',
    `read pid started < ${CONVEX_PROCESS_RECORD}; `
    + 'case "$pid:$started" in *[!0-9:]*) exit 4;; esac; '
    + '[ "$pid" -gt 1 ] && [ "$started" -gt 0 ]; '
    + 'stat=$(cat /proc/$pid/stat); rest=${stat##*) }; set -- $rest; '
    + '[ "$3" = "$pid" ] && [ "${20}" = "$started" ] '
    + '|| { echo "Convex process identity changed" >&2; exit 4; }; printf "%s" "$pid"']);
  updateBackendLease(leasePath, { token: leaseToken }, next => { next.state = 'restarting'; return next; });
  attemptDocker(['exec', container.id, 'sh', '-ec', hostedRecordedProcessStopScript(CONVEX_PROCESS_RECORD)]);
  attemptDocker(['exec', container.id, 'sh', '-ec', `test ! -e /proc/${pid}`]);
  await waitFor(async () => !(await answers(`${lease.resources.serverUri}/version`))
    && !(await answers(`http://127.0.0.1:${ports.express}/`)), 10_000, 'Convex API and site to stop');
  if (mode === 'reset') {
    attemptDocker(['exec', container.id, 'sh', '-ec',
      'test "$(readlink -f /convex/data)" = /convex/data; '
      + 'find /convex/data -mindepth 1 -maxdepth 1 -exec rm -rf -- {} +']);
  }
  startConvexProcess(lease, ports.express!);
  updateBackendLease(leasePath, { token: leaseToken }, next => { next.state = 'active'; return next; });
}

export function releaseConvex(leasePath: string, leaseToken: string): boolean {
  readBackendLease(leasePath, { token: leaseToken, backend: 'convex' });
  return releaseBackendLease(leasePath, leaseToken, { hostTeardown: stopHostedHost });
}

export async function resetConvex(): Promise<void> {
  const { path, lease } = leaseFromEnv(process.env, { backend: 'convex', active: true });
  await controlConvex({ leasePath: path, leaseToken: lease.ownershipToken,
    ports: portsFor(loadTrack(lease.track), 'convex', lease.runIndex), mode: 'reset' });
}

export async function controlConvexApplication(input: StackLifecycleInput): Promise<void> {
  if (input.mode !== 'restart') throw new Error('Convex backend control supports restart only');
  input.signal?.throwIfAborted();
  const { path, lease } = leaseFromEnv(process.env, { backend: 'convex', active: true });
  if (lease.runId !== input.lease.runId || lease.ownershipToken !== input.lease.ownershipToken) {
    throw new Error('Convex runtime control lease changed');
  }
  await controlConvex({ leasePath: path, leaseToken: lease.ownershipToken,
    ports: portsFor(loadTrack(lease.track), 'convex', lease.runIndex), mode: 'restart' });
  input.signal?.throwIfAborted();
}
