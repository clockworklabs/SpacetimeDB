import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import type { SpawnSyncOptionsWithStringEncoding } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { inspectBuildContainer } from '../container/build-container-inspection.js';
import { DEFAULT_BUILD_IMAGE } from '../src/composition/product-config.js';
import { createBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';
import { compiledEntrypoint } from '../src/package-root.js';

const required = <T>(value: T | null | undefined, description: string): T => {
  if (value === null || value === undefined) throw new Error(`${description} is required`);
  return value;
};
const record = (text: string, description: string): Record<string, unknown> => {
  const value: unknown = JSON.parse(text);
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${description} must be an object`);
  }
  return value as Record<string, unknown>;
};
const stringField = (value: Record<string, unknown>, field: string, description: string): string => {
  const result = value[field];
  if (typeof result !== 'string') throw new Error(`${description}.${field} must be a string`);
  return result;
};

test('Docker replaces a stopped leased build container and preserves its app mount', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-recovery-docker-'));
  const app = join(root, 'app');
  const leasePath = join(root, 'lease.json');
  const containerName = `stack-bench-${required(root.split(/[\\/]/).at(-1), 'temporary directory name')}`;
  const docker = (args: readonly string[],
    options: Omit<SpawnSyncOptionsWithStringEncoding, 'encoding'> = {}) => spawnSync('docker', args,
    { encoding: 'utf8', timeout: 120_000, ...options });
  try {
    mkdirSync(app);
    const databaseContainer = docker(['inspect', '--format', '{{.Id}}', 'stack-bench-mongodb']);
    assert.equal(databaseContainer.status, 0, databaseContainer.stderr);
    const lease = createBackendLease({ runId: 'recover-build-docker-smoke', backend: 'mongodb',
      track: 'ecommerce', runIndex: 91, database: 'app_ecom_run91',
      container: { name: 'stack-bench-mongodb', id: required(databaseContainer.stdout,
        'database container identifier').trim() } });
    lease.state = 'active';
    writeBackendLease(leasePath, lease);
    const env = { ...process.env, STACK_BENCH_LEASE: leasePath,
      STACK_BENCH_LEASE_TOKEN: lease.ownershipToken };
    const build = compiledEntrypoint('container', 'run-build.js');
    const baseArgs = [build, '--prepare-only', '--app', app, '--backend', 'mongodb', '--image',
      process.env.STACK_BENCH_BUILD_IMAGE ?? DEFAULT_BUILD_IMAGE];
    const runBuild = (args: readonly string[]) => spawnSync(process.execPath, args,
      { encoding: 'utf8', env, timeout: 180_000 });

    const first = runBuild(baseArgs);
    assert.equal(first.status, 0, first.stderr);
    const firstResult = record(required(first.stdout, 'first build result'), 'first build result');
    const firstContainerName = stringField(firstResult, 'containerName', 'first build result');
    const firstIdentity = stringField(firstResult, 'identity', 'first build result');
    const inspected = inspectBuildContainer(firstContainerName);
    assert(inspected);
    assert.equal(inspected.readonlyRootfs, true);
    assert.deepEqual([...inspected.capAdd].sort(),
      ['CHOWN', 'DAC_OVERRIDE', 'FOWNER', 'KILL', 'SETGID', 'SETUID']);
    assert(inspected.capDrop.includes('ALL'));
    assert(inspected.securityOpt.includes('no-new-privileges'));
    assert.equal(inspected.pidsLimit, 512);
    assert.equal(inspected.tmpfs['/tmp'], 'rw,nosuid,nodev,mode=1777');
    assert.equal(inspected.tmpfs['/home/developer'],
      'rw,nosuid,nodev,uid=10001,gid=10001,mode=0700');
    assert.equal(inspected.tmpfs['/home/developer/.claude'],
      'rw,nosuid,nodev,uid=10001,gid=10001,mode=0700');
    assert.equal(inspected.tmpfs['/deps'], 'rw,exec,nosuid,nodev,mode=0755');
    assert.equal(inspected.tmpfs['/run/application'], 'rw,nosuid,nodev,mode=0700');
    assert.equal(inspected.tmpfs['/root'], undefined);
    const agentCaps = docker(['exec', '--user', '10001:10001', firstContainerName,
      'sh', '-c', 'grep "^CapEff:" /proc/self/status']);
    assert.equal(agentCaps.status, 0, agentCaps.stderr);
    assert.match(agentCaps.stdout, /CapEff:\s+0+\s*$/);
    writeFileSync(join(app, 'preserved.txt'), 'preserved\n');
    assert.equal(docker(['stop', firstContainerName]).status, 0);

    const second = runBuild([build, '--recover-stopped-container', ...baseArgs.slice(1)]);
    assert.equal(second.status, 0, second.stderr);
    const secondResult = record(required(second.stdout, 'second build result'), 'second build result');
    const secondContainerName = stringField(secondResult, 'containerName', 'second build result');
    const secondIdentity = stringField(secondResult, 'identity', 'second build result');
    assert.notEqual(firstIdentity, secondIdentity);
    const mounted = docker(['exec', secondContainerName, 'cat', '/app/preserved.txt']);
    assert.equal(mounted.status, 0, mounted.stderr);
    assert.equal(mounted.stdout.trim(), 'preserved');
    const reused = runBuild(baseArgs);
    assert.equal(reused.status, 0, reused.stderr);
    assert.equal(stringField(record(required(reused.stdout, 'reused build result'),
      'reused build result'), 'identity', 'reused build result'), secondIdentity);
  } finally {
    const exact = docker(['inspect', '--format', '{{.Id}}', containerName]);
    if (exact.status === 0 && exact.stdout.trim()) docker(['rm', '-f', exact.stdout.trim()]);
    rmSync(root, { recursive: true, force: true });
  }
});
