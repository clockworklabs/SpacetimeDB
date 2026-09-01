#!/usr/bin/env node
// Build a temporary fixture copy inside the benchmark image and lease boundary.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { writeRunJson } from '../evidence/artifacts.js';
import type { BackendLease } from '../runtime/backend-lease.js';
import { acquireResourceLocks, backendResourceLockKeys, createBackendLease,
  publicBackendLease, readBackendLease, releaseResourceLocks, resourceLockScope,
  updateBackendLease, writeBackendLease } from '../runtime/backend-lease.js';
import { resolveContainerImage } from '../runtime/container-image.js';
import type { ResolvedContainerImage } from '../runtime/container-image.js';
import { codingContainerAgentCommand, codingContainerAgentExecOptions }
  from '../runtime/coding-container-policy.js';
import { executeStackCapability } from '../stacks/stack-adapter-contract.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';
import { DEFAULT_BUILD_IMAGE } from '../composition/product-config.js';
import { loadTrack } from '../composition/tracks.js';
import { inspectImportedReference, loadReferenceRegistry,
  prepareReferenceFixtureSource, referenceMetadataIssues,
  validateReferenceRegistry } from './reference-fixtures.js';
import { referenceInstallSteps } from './reference-install.js';

import { STACK_BENCH_ROOT as ROOT, compiledEntrypoint } from '../package-root.js';
const RUN_BUILD = compiledEntrypoint('container', 'run-build.js');
const IMAGE = process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE;

import type { ReferenceFixture } from './reference-fixtures.js';
import type { ReferenceInstallMetadata } from './reference-install.js';

interface BuildCommand {
  cwd: string;
  command: string;
  ok: boolean;
  durationMs: number;
  outputTail: string;
}

interface FixtureBuild {
  id: string;
  backend: string;
  ok: boolean;
  durationMs: number;
  error: string | null;
  commands: BuildCommand[];
  [key: string]: unknown;
}

type ImageIdentity = ResolvedContainerImage;

type ReferenceMetadataForBuild = ReferenceInstallMetadata & {
  kind: string;
  server: { directory: string };
  client: { directory: string };
  moduleDirectory: string;
  bindingsDirectory: string;
};

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object';

// A failed child process carries its output on the error.
const streams = (error: unknown, ...keys: readonly string[]): string =>
  record(error) ? keys.map(key => String(error[key] ?? '')).join('') : '';

const errorDetail = (error: unknown): string =>
  error instanceof Error ? error.stack ?? error.message : String(error);


function parseArgs(argv: readonly string[]): {
  backend: string | null; fixture: string | null; out: string | null;
} {
  const args: { backend: string | null; fixture: string | null; out: string | null } =
    { backend: null, fixture: null, out: null };
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--backend') args.backend = argv[++i] ?? null;
    else if (argv[i] === '--fixture') args.fixture = argv[++i] ?? null;
    else if (argv[i] === '--out') args.out = resolve(argv[++i] ?? '');
    else throw new Error(`unknown argument ${argv[i]}`);
  }
  if (args.backend && !STACK_ADAPTER_REGISTRY.ids.includes(args.backend)) {
    throw new Error(`unknown backend ${args.backend}`);
  }
  return args;
}

function containerIdentity(name: string): { name: string; id: string } {
  const output = execFileSync('docker', ['inspect', '--format', '{{.Id}} {{.State.Running}}', name],
    { encoding: 'utf8', stdio: 'pipe' }).trim();
  const [id, running] = output.split(/\s+/);
  if (!id || !/^[a-f0-9]{64}$/.test(id) || running !== 'true') {
    throw new Error(`${name} is not a running Docker container`);
  }
  return { name, id };
}

function run(container: string, cwd: string, command: string,
  args: readonly string[], commands: BuildCommand[]): void {
  const started = Date.now();
  const printable = [command, ...args].join(' ');
  try {
    const output = execFileSync('docker', ['exec', ...codingContainerAgentExecOptions(),
      '-w', cwd, container, ...codingContainerAgentCommand(command, args)],
      { encoding: 'utf8', stdio: 'pipe', maxBuffer: 64 * 1024 * 1024 });
    commands.push({ cwd, command: printable, ok: true, durationMs: Date.now() - started,
      outputTail: output.slice(-8_192) });
  } catch (error) {
    const output = streams(error, 'stdout', 'stderr');
    commands.push({ cwd, command: printable, ok: false, durationMs: Date.now() - started,
      outputTail: output.slice(-16_384) });
    throw new Error(`${printable} failed in ${cwd}:\n${output.slice(-16_384)}`);
  }
}

function buildCommands(metadata: ReferenceMetadataForBuild, container: string,
  commands: BuildCommand[]): void {
  for (const directory of metadata.installDirectories) {
    run(container, `/app/${directory}`, 'node', ['-e',
      "const fs=require('node:fs'); for(const f of ['package.json','package-lock.json']) if(!fs.existsSync(f)) throw new Error(`${process.cwd()}/${f} is missing`);"], commands);
  }
  for (const step of referenceInstallSteps(metadata)) {
    run(container, `/app/${step.directory}`, step.command, step.args, commands);
  }
  if (metadata.kind === 'node-api') {
    run(container, `/app/${metadata.server.directory}`, 'npm', ['exec', 'tsc', '--', '--noEmit'], commands);
    run(container, `/app/${metadata.client.directory}`, 'npm', ['run', 'build'], commands);
    return;
  }
  run(container, `/app/${metadata.moduleDirectory}`, '/deps/spacetimedb-cli',
    ['build', '--module-path', `/app/${metadata.moduleDirectory}`], commands);
  run(container, `/app/${metadata.moduleDirectory}`, '/deps/spacetimedb-cli',
    ['generate', '--lang', 'typescript', '--module-path', `/app/${metadata.moduleDirectory}`,
      '--out-dir', `/app/${metadata.bindingsDirectory}`, '--yes', '--no-config'], commands);
  run(container, `/app/${metadata.client.directory}`, 'npm', ['run', 'build'], commands);
}

function removeLeasedBuildContainer(leasePath: string, token: string): void {
  const lease = readBackendLease(leasePath, { token });
  const container = lease.resources.buildContainer;
  if (!container) return;
  let actual: string;
  try {
    actual = execFileSync('docker', ['inspect', '--format', '{{.Id}}', container.name],
      { encoding: 'utf8', stdio: 'pipe' }).trim();
  } catch (error) {
    if (/No such (object|container)/i.test(streams(error, 'stderr', 'message'))) return;
    throw error;
  }
  if (actual !== container.id) throw new Error(`refusing to remove ${container.name}: lease id does not match`);
  execFileSync('docker', ['rm', '-f', container.id], { stdio: 'pipe' });
}

function qualify(fixture: ReferenceFixture, imageIdentity: ImageIdentity): FixtureBuild {
  const started = Date.now();
  const work = mkdtempSync(join(tmpdir(), `stack-bench-reference-${fixture.backend}-`));
  const app = join(work, 'app');
  const leasePath = join(work, 'backend-lease.json');
  const commands: BuildCommand[] = [];
  let lease: BackendLease | undefined;
  let leaseEvidence: unknown = null;
  let error: string | null = null;
  let cleanupComplete = true;
  const recordError = (message: string): void => {
    error = error ? `${error}\n${message}` : message;
  };
  try {
    prepareReferenceFixtureSource(fixture, app);
    const adapter = STACK_ADAPTER_REGISTRY.get(fixture.backend);
    const preparedLease = executeStackCapability(adapter, 'lease', 'prepare', {
      track: loadTrack(fixture.track), runIndex: 0, runtimeDir: work,
      serverUri: 'http://127.0.0.1:1', env: process.env,
      helpers: {
        moduleName: () => `reference-${fixture.track}`,
        dbName: () => `reference_${fixture.track}`,
        containerIdentity,
      },
    });
    const prepared_ = record(preparedLease) ? preparedLease : {};
    const preparedResources = record(prepared_.lease) ? prepared_.lease : {};
    const lockKeys = Array.isArray(prepared_.lockKeys) ? prepared_.lockKeys : [];
    lease = createBackendLease({ runId: basename(work), backend: fixture.backend,
      track: fixture.track, runIndex: 0,
      ...preparedResources });
    lease.resources.locks.push(...acquireResourceLocks({ ...resourceLockScope(),
      keys: backendResourceLockKeys(lease, lockKeys), lease }));
    lease.state = 'active';
    writeBackendLease(leasePath, lease);
    const prepared = execFileSync(process.execPath,
      [RUN_BUILD, '--app', app, '--backend', fixture.backend, '--image', IMAGE, '--prepare-only'],
      { encoding: 'utf8', stdio: 'pipe', maxBuffer: 64 * 1024 * 1024,
        env: { ...process.env, STACK_BENCH_LEASE: leasePath, STACK_BENCH_LEASE_TOKEN: lease.ownershipToken } });
    const identity: unknown = JSON.parse(prepared.trim().split(/\r?\n/).pop() ?? '');
    if (!record(identity) || typeof identity.identity !== 'string'
      || typeof identity.containerName !== 'string') {
      throw new Error('prepared container did not report its identity');
    }
    const active = readBackendLease(leasePath, { token: lease.ownershipToken, backend: fixture.backend, active: true });
    if (active.resources.buildContainer?.id !== identity.identity.split(' ')[0]) {
      throw new Error('prepared container identity was not recorded in the lease');
    }
    const metadata: ReferenceMetadataForBuild = JSON.parse(
      execFileSync('docker', ['exec', identity.containerName,
        'cat', '/app/reference.json'], { encoding: 'utf8', stdio: 'pipe' }));
    const metadataIssues = referenceMetadataIssues(metadata);
    if (metadataIssues.length) throw new Error(metadataIssues.join('; '));
    buildCommands(metadata, identity.containerName, commands);
  } catch (caught) {
    error = errorDetail(caught);
  } finally {
    if (lease) {
      try {
        if (existsSync(leasePath)) {
          removeLeasedBuildContainer(leasePath, lease.ownershipToken);
          updateBackendLease(leasePath, { token: lease.ownershipToken, backend: lease.backend, runId: lease.runId }, next => {
            if (next.resources.buildContainer) next.resources.buildContainer.running = false;
            next.state = 'released';
            next.releasedAt = new Date().toISOString();
            return next;
          });
        }
      }
      catch (cleanupError) {
        cleanupComplete = false;
        recordError(`cleanup failed: ${errorDetail(cleanupError)}`);
      }
      try {
        const finalLease = existsSync(leasePath)
          ? readBackendLease(leasePath, { token: lease.ownershipToken })
          : lease;
        if (cleanupComplete) {
          releaseResourceLocks(finalLease);
          for (const lock of finalLease.resources.locks) lock.releasedAt = new Date().toISOString();
        }
        leaseEvidence = publicBackendLease(finalLease);
      }
      catch (cleanupError) {
        cleanupComplete = false;
        recordError(`lock cleanup failed: ${errorDetail(cleanupError)}`);
      }
    }
    if (cleanupComplete) rmSync(work, { recursive: true, force: true });
    else recordError(`recovery authority retained at ${leasePath}`);
  }
  return { id: fixture.id, backend: fixture.backend, ok: error === null,
    fixtureSha256: fixture.imported?.sourceSha256, image: imageIdentity,
    durationMs: Date.now() - started, commands, backendLease: leaseEvidence, error };
}

async function main(): Promise<void> {
  const args = parseArgs(process.argv);
  const registry = loadReferenceRegistry();
  const validation = validateReferenceRegistry(registry);
  if (!validation.ok) throw new Error(`reference registry is invalid:\n${validation.issues.join('\n')}`);
  const fixtures = registry.fixtures.filter(fixture => fixture.status !== 'blocked')
    .filter(fixture => !args.backend || fixture.backend === args.backend)
    .filter(fixture => !args.fixture || fixture.id === args.fixture);
  if (!fixtures.length) throw new Error('no imported fixtures matched');
  for (const fixture of fixtures) {
    const inspection = inspectImportedReference(fixture);
    if (!inspection.ok) throw new Error(`${fixture.id} import is invalid:\n${inspection.failures.join('\n')}`);
  }
  const imageIdentity = resolveContainerImage(IMAGE);
  const id = `reference-build-${new Date().toISOString().replace(/[-:.TZ]/g, '').slice(0, 14)}-${process.pid}`;
  const artifact: {
    id: string; kind: string; startedAt: string; isolation: string;
    image: ImageIdentity; fixtures: FixtureBuild[]; completedAt?: string; ok?: boolean;
  } = { id, kind: 'reference_build', startedAt: new Date().toISOString(),
    isolation: 'docker', image: imageIdentity, fixtures: [] };
  for (const fixture of fixtures) {
    console.log(`building ${fixture.id} in ${imageIdentity.id}`);
    artifact.fixtures.push(qualify(fixture, imageIdentity));
  }
  artifact.completedAt = new Date().toISOString();
  artifact.ok = artifact.fixtures.every(fixture => fixture.ok);
  const out = args.out ?? join(ROOT, 'results', 'reference-builds', `${id}.json`);
  writeRunJson(out, artifact);
  console.log(JSON.stringify({ ok: artifact.ok, artifact: out,
    fixtures: artifact.fixtures.map(({ id: fixtureId, ok, durationMs, error }) => ({ id: fixtureId, ok, durationMs,
      error: error ? error.split(/\r?\n/)[0] : null })) }, null, 2));
  if (!artifact.ok) process.exitCode = 1;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error: unknown) => {
    console.error(error instanceof Error ? error.stack ?? error.message : String(error));
    process.exitCode = 2;
  });
}
