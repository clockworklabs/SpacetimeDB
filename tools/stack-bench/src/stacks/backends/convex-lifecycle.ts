import { createHash } from 'node:crypto';
import { backendResourceLockKeys, readBackendLease, updateBackendLease, verifyResourceLocks }
  from '../../runtime/backend-lease.js';
import { releaseBackendLease } from '../../runtime/backend-teardown.js';
import { CODING_CONTAINER_CONTROL_DIR } from '../../runtime/coding-container-policy.js';
import { attemptDocker, createAttemptNetwork, createAttemptContainer, installAttemptFirewall,
  createAttemptBrowser } from '../../runtime/docker-network.js';
import { sleepSync } from '../../runtime/platform.js';
import type { StackRunPorts } from '../stack-adapter-contract.js';
import { stopHostedHost } from '../stack-teardown-operations.js';

export const CONVEX_BACKEND_IMAGE = 'ghcr.io/get-convex/convex-backend@sha256:afbf4292df387c8f031a68d00048551cf1640ddf0013c51ac704a89d7e73e743';

// Private lifecycle slice. This module does not make Convex a selectable stack.
export function activateConvex({ leasePath, leaseToken, ports }: {
  leasePath: string; leaseToken: string; ports: StackRunPorts;
}): void {
  if (process.platform !== 'linux') throw new Error('Convex activation requires the Linux Docker controller');
  const lease = readBackendLease(leasePath, { token: leaseToken, backend: 'convex' });
  if (lease.state !== 'created' || lease.resources.container || lease.resources.creationIntents) {
    throw new Error('Convex activation requires a fresh lease');
  }
  if (ports.express === null || ports.dbPort !== null) throw new Error('Convex requires frontend, native API and HTTP-action ports');
  const nativePort = Number(new URL(lease.resources.serverUri!).port);
  const endpoints = [ports.vite, ports.express, nativePort];
  if (new Set(endpoints).size !== endpoints.length) throw new Error('Convex endpoint ports must differ');
  const required = backendResourceLockKeys(lease, ports);
  if (required.some(key => !lease.resources.locks.some(lock => lock.key === key && !lock.releasedAt))) {
    throw new Error('Convex endpoint resources must be claimed before activation');
  }
  verifyResourceLocks(lease);
  updateBackendLease(leasePath, { token: leaseToken }, next => { next.state = 'starting'; return next; });
  const image = attemptDocker(['image', 'inspect', '--format', '{{.Id}}', CONVEX_BACKEND_IMAGE]);
  let current = createAttemptNetwork(leasePath, lease);
  const container = createAttemptContainer(leasePath, current, 'backend', image, current.resources.network!.id,
    endpoints.flatMap(port => ['--publish', `127.0.0.1:${port}:${port}`]));
  current = installAttemptFirewall(leasePath, readBackendLease(leasePath, { token: leaseToken }));
  const instance = `sb-${createHash('sha256').update(lease.runId).digest('hex').slice(0, 20)}`;
  // The pinned vendor helper generates and stores the secret in this container's
  // anonymous /convex/data volume. It never enters command arguments or evidence.
  attemptDocker(['exec', '-d', container.id, 'bash', '-ec',
    `umask 077; mkdir -p ${CODING_CONTAINER_CONTROL_DIR} /convex/data/tmp /convex/data/storage; `
    + `export INSTANCE_NAME=${instance}; source ./read_credentials.sh; export TMPDIR=/convex/data/tmp; `
    + 'exec setsid ./convex-local-backend --instance-name "$INSTANCE_NAME" --instance-secret "$INSTANCE_SECRET" '
    + `--port ${nativePort} --site-proxy-port ${ports.express} `
    + `--convex-origin http://127.0.0.1:${nativePort} --convex-site http://127.0.0.1:${ports.express} `
    + '--local-storage /convex/data/storage --disable-beacon --do-not-require-ssl /convex/data/db.sqlite3'
    + ` > ${CODING_CONTAINER_CONTROL_DIR}/stack-bench-backend.log 2>&1`]);
  const deadline = Date.now() + 60_000;
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
  createAttemptBrowser(leasePath, current);
  updateBackendLease(leasePath, { token: leaseToken }, next => { next.state = 'active'; return next; });
}

export function releaseConvex(leasePath: string, leaseToken: string): boolean {
  readBackendLease(leasePath, { token: leaseToken, backend: 'convex' });
  return releaseBackendLease(leasePath, leaseToken, { hostTeardown: stopHostedHost });
}
