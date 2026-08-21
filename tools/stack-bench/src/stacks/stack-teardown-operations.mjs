import { readBackendLease, updateBackendLease } from '../runtime/backend-lease.mjs';
import { killDetachedTree, killTree, pidsOnPort, sleepSync } from '../runtime/platform.mjs';

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
  if (lease.state === 'created') {
    const url = new URL(lease.resources.serverUri);
    const port = Number(url.port);
    const actual = pidsOnPort(port, { strict: true });
    if (lease.resources.launchedPid || lease.resources.listenerPids?.length || actual.length) {
      console.error(`  REFUSED to release unactivated lease ${lease.runId}: :${port} is not empty`);
      return false;
    }
    updateBackendLease(leasePath,
      { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
        next.state = 'stopped';
        next.stoppedAt ??= new Date().toISOString();
        return next;
      });
    return true;
  }
  if (lease.state === 'starting') {
    const url = new URL(lease.resources.serverUri);
    const port = Number(url.port);
    if (!lease.resources.launchedPid) {
      console.error(`  REFUSED to release starting lease ${lease.runId}: launched PID was not recorded`);
      return false;
    }
    killDetachedTree(lease.resources.launchedPid);
    for (let i = 0; i < 40 && pidsOnPort(port, { strict: true }).length; i++) sleepSync(250);
    const remaining = pidsOnPort(port, { strict: true });
    if (remaining.length) {
      console.error(`  REFUSED to release starting lease ${lease.runId}: :${port} still has listener PID ${remaining.join(', ')}`);
      return false;
    }
    updateBackendLease(leasePath,
      { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
        next.resources.listenerPids = [];
        next.state = 'stopped';
        next.stoppedAt ??= new Date().toISOString();
        return next;
      });
    return true;
  }
  if (!['active', 'restarting', 'retained'].includes(lease.state)) {
    console.error(`  REFUSED to stop SpacetimeDB from lease state ${lease.state}`);
    return false;
  }
  const url = new URL(lease.resources.serverUri);
  if (!['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname) || !url.port) {
    console.error(`  REFUSED to stop non-loopback SpacetimeDB target ${lease.resources.serverUri}`);
    return false;
  }
  const port = Number(url.port);
  if (lease.state === 'restarting' && lease.resources.launchedPid) {
    // During restart, launchedPid is the exact process tree currently being
    // replaced. Kill that tree first; anything still listening afterward is
    // unproven and must be left alone.
    killDetachedTree(lease.resources.launchedPid);
    for (let i = 0; i < 40 && pidsOnPort(port, { strict: true }).length; i++) sleepSync(250);
    const remaining = pidsOnPort(port, { strict: true });
    if (remaining.length) {
      console.error(`  REFUSED to release restarting lease ${lease.runId}: :${port} still has listener PID ${remaining.join(', ')}`);
      return false;
    }
    updateBackendLease(leasePath,
      { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
        next.resources.listenerPids = [];
        next.state = 'stopped';
        next.stoppedAt ??= new Date().toISOString();
        return next;
      });
    return true;
  }
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
  if (lease.resources.launchedPid) killDetachedTree(lease.resources.launchedPid);
  updateBackendLease(leasePath,
    { token: leaseToken, backend: lease.backend, runId: lease.runId }, next => {
      next.resources.listenerPids = [];
      next.state = 'stopped';
      next.stoppedAt ??= new Date().toISOString();
      return next;
    });
  return true;
}
