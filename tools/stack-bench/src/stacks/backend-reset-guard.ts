import type { TextCommandExecutor } from '../runtime/command-executor.js';
import type { BackendLease } from '../runtime/backend-lease.js';

interface LeasedContainer {
  id: string;
  name: string;
}

export interface LeasedDatabase {
  resources: { container: LeasedContainer; database: string };
}

export interface LeasedSpacetime {
  resources: { module: string; serverUri: string };
}

export function requireLeasedDatabase(lease: BackendLease): LeasedDatabase {
  const { container, database } = lease.resources;
  if (!container || !database) throw new Error(`${lease.backend} lease has no database target`);
  return { resources: { container, database } };
}

export function requireLeasedSpacetime(lease: BackendLease): LeasedSpacetime {
  const { module, serverUri } = lease.resources;
  if (!module || !serverUri) throw new Error('SpacetimeDB lease has no module target');
  return { resources: { module, serverUri } };
}

export function assertLeasedContainer(
  container: LeasedContainer,
  exec: TextCommandExecutor,
  timeoutMs: number,
  purpose: string,
): string {
  const actual = exec('docker', ['inspect', '--format', '{{.Id}}', container.name],
    { encoding: 'utf8', stdio: 'pipe', timeout: timeoutMs }).trim();
  if (actual !== container.id) {
    throw new Error(`${container.name} changed after lease creation; refusing ${purpose}`);
  }
  return actual;
}
