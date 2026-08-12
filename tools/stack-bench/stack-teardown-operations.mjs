import { readBackendLease, updateBackendLease } from './backend-lease.mjs';
import { killTree, pidsOnPort, sleepSync } from './platform.mjs';

export function stopHostedHost({ leasePath, leaseToken, lease, retainHost = false }) {
  updateBackendLease(leasePath,
    { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
      next.state = retainHost ? 'retained' : 'stopped';
      if (!retainHost) next.stoppedAt ??= new Date().toISOString();
      return next;
    });
  return true;
}

export function stopSpacetimeHost({ leasePath, leaseToken, retainHost = false }) {
  const lease = readBackendLease(leasePath, { token: leaseToken, backend: 'spacetime' });
  if (retainHost) {
    updateBackendLease(leasePath,
      { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
        next.state = 'retained';
        return next;
      });
    return true;
  }
  if (['stopped', 'released'].includes(lease.state)) return true;
  if (!['active', 'restarting'].includes(lease.state)) {
    console.error(`  REFUSED to stop SpacetimeDB from lease state ${lease.state}`);
    return false;
  }
  const url = new URL(lease.resources.serverUri);
  if (!['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname) || !url.port) {
    console.error(`  REFUSED to stop non-loopback SpacetimeDB target ${lease.resources.serverUri}`);
    return false;
  }
  const port = Number(url.port);
  const actual = pidsOnPort(port, { strict: true });
  const expected = new Set((lease.resources.listenerPids ?? []).map(String));
  const unexpected = actual.filter(pid => !expected.has(String(pid)));
  if (unexpected.length) {
    console.error(`  REFUSED to stop :${port}: listener PID ${unexpected.join(', ')} is not in lease ${lease.runId}`);
    return false;
  }
  for (const pid of actual) {
    killTree(pid);
    if (pidsOnPort(port, { strict: true }).includes(String(pid))) {
      try { process.kill(Number(pid), 'SIGKILL'); } catch { /* already gone */ }
    }
  }
  for (let i = 0; i < 40 && pidsOnPort(port, { strict: true }).length; i++) sleepSync(250);
  const remaining = pidsOnPort(port, { strict: true });
  if (remaining.length) {
    console.error(`  REFUSED to release lease ${lease.runId}: :${port} still has listener PID ${remaining.join(', ')}`);
    return false;
  }
  if (actual.length) console.log(`  stopped the SpacetimeDB host this run started on :${port}`);
  if (lease.resources.launchedPid) killTree(lease.resources.launchedPid);
  updateBackendLease(leasePath,
    { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
      next.resources.listenerPids = [];
      next.state = 'stopped';
      next.stoppedAt ??= new Date().toISOString();
      return next;
    });
  return true;
}
