import { execFileSync } from 'node:child_process';
import { join } from 'node:path';
import { CODING_PROVIDERS } from '../../container/coding-providers.js';
import { hashDirectory, sha256 } from '../evidence/provenance.js';
import { leaseFromEnv } from '../runtime/backend-lease.js';
import { STACK_BENCH_ROOT } from '../package-root.js';
import { hashAppSource } from '../runtime/source-snapshot.js';

type Json = Record<string, unknown>;
const record = (value: unknown): value is Json => value !== null && typeof value === 'object' && !Array.isArray(value);

/** A live-state binding, never a recoverable on-disk checkpoint. No secrets enter the digest. */
export function captureNativeContinuation({ appDir, provider, sessionId, model, imageId,
  env = process.env }: { appDir: string; provider: string; sessionId: string; model: string;
    imageId: string; env?: NodeJS.ProcessEnv }): string {
  const adapter = CODING_PROVIDERS[provider as keyof typeof CODING_PROVIDERS];
  if (!adapter) throw new Error('Unknown coding provider');
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(sessionId)) {
    throw new Error('Missing valid native session identity');
  }
  const { lease } = leaseFromEnv(env, { active: true });
  const build = lease.resources.buildContainer;
  const network = lease.resources.network?.namespaceContainerId;
  if (!build?.id || !network) throw new Error('Continuation requires the same isolated Docker runtime');
  const ids = [...new Set([build.id, network, lease.resources.container?.id].filter((id): id is string => !!id))];
  if (ids.some(id => !/^[a-f0-9]{64}$/.test(id))) throw new Error('Runtime container identity is incomplete');
  const containers: unknown = JSON.parse(execFileSync('docker', ['inspect', ...ids], {
    env, encoding: 'utf8', timeout: 10_000, maxBuffer: 8 * 1024 * 1024, windowsHide: true,
  }));
  if (!Array.isArray(containers) || containers.length !== ids.length) throw new Error('Runtime inspection is incomplete');
  const identities = containers.map((value: unknown, index: number) => {
    if (!record(value) || value.Id !== ids[index] || !record(value.State)
      || value.State.Running !== true || value.State.Paused === true || value.State.OOMKilled === true
      || (record(value.State.Health) && value.State.Health.Status !== 'healthy')) {
      throw new Error('Required runtime is no longer running and healthy');
    }
    if (value.Id === build.id && value.Image !== imageId) throw new Error('Build image changed');
    return { id: value.Id, image: value.Image, startedAt: value.State.StartedAt, restarts: value.RestartCount };
  });
  const directory = adapter.projects(appDir);
  adapter.validateContinuation(directory, sessionId, model);
  const native = hashDirectory(directory, { exclude: (_name, entry) => {
    if (entry.isSymbolicLink()) throw new Error('Native session contains a symbolic link');
    return false;
  } });
  return sha256(JSON.stringify({ provider, sessionId, model, imageId, runId: lease.runId,
    leaseCreatedAt: lease.createdAt, backend: lease.backend, database: lease.resources.database,
    module: lease.resources.module,
    executable: hashDirectory(join(STACK_BENCH_ROOT, 'dist'), { exclude: name => name.startsWith('tests/') }).sha256,
    containers: identities, native: native.sha256, source: hashAppSource(appDir).sha256 }));
}
