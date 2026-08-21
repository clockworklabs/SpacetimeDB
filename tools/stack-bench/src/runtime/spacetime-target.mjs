// Resolve SpacetimeDB control targets only from the authenticated run lease.
// Generated client config is application input and must never select a module,
// host, or container for grader operations.

import { execFileSync } from 'node:child_process';
import { leaseFromEnv } from './backend-lease.mjs';

export function containerReachableSpacetimeUri(lease, networkMode = null) {
  const mode = networkMode ?? lease.resources.buildContainer?.networkMode ?? 'bridge';
  if (mode === 'host') return lease.resources.serverUri;
  if (mode !== 'bridge') throw new Error(`unsupported build container network mode ${mode}`);
  return lease.resources.serverUri
    .replace('127.0.0.1', 'host.docker.internal')
    .replace('localhost', 'host.docker.internal');
}

export function leasedSpacetimeTarget({ requireBuildContainer = false, exec = execFileSync } = {}) {
  const { lease } = leaseFromEnv(process.env, { backend: 'spacetime', active: true });
  const target = {
    mod: lease.resources.module,
    uri: lease.resources.serverUri,
    containerUri: containerReachableSpacetimeUri(lease),
    buildContainer: null,
  };
  if (!requireBuildContainer) return target;
  const container = lease.resources.buildContainer;
  if (!container || container.running !== true) {
    throw new Error('leased build container is not active');
  }
  const actual = exec('docker', ['inspect', '--format', '{{.Id}}', container.name],
    { encoding: 'utf8', stdio: 'pipe', timeout: 120_000 }).trim();
  if (actual !== container.id) {
    throw new Error(`${container.name} changed after lease creation; refusing SpacetimeDB control`);
  }
  target.buildContainer = container;
  return target;
}
