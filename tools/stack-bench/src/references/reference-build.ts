#!/usr/bin/env node
// Build a temporary fixture copy inside the benchmark image and lease boundary.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { basename, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { parseArgs as parseNodeArgs } from 'node:util';
import { ARTIFACT_FILE, writeRunJson } from '../evidence/artifacts.js';
import type { BackendLease } from '../runtime/backend-lease.js';
import { claimBackendResources, backendResourceLockKeys, createBackendLease,
  publicBackendLease, readBackendLease, resourceLockScope } from '../runtime/backend-lease.js';
import { releaseBackendLease } from '../runtime/backend-teardown.js';
import { redactCredentials } from '../evidence/diagnostic-sanitizer.js';
import { resolveContainerImage } from '../runtime/container-image.js';
import type { ResolvedContainerImage } from '../runtime/container-image.js';
import { runningContainerIdentity } from '../runtime/container-identity.js';
import { CODING_CONTAINER_APP_ROOT, CODING_CONTAINER_SPACETIME_CLI,
  codingContainerAgentCommand, codingContainerAgentExecOptions }
  from '../runtime/coding-container-policy.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';
import { DEFAULT_BUILD_IMAGE } from '../composition/product-config.js';
import { loadTrack, portsFor } from '../composition/tracks.js';
import { inspectImportedReference, loadReferenceRegistry,
  prepareReferenceFixtureSource, REFERENCE_METADATA_FILE, referenceMetadataIssues,
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
  redactCredentials(error instanceof Error ? error.stack ?? error.message : String(error));


function parseArgs(argv: readonly string[]): {
  backend: string | null; fixture: string | null; out: string | null;
} {
  const { values } = parseNodeArgs({ args: [...argv.slice(2)], options: {
    backend: { type: 'string' }, fixture: { type: 'string' }, out: { type: 'string' },
  } });
  const args = { backend: values.backend ?? null, fixture: values.fixture ?? null,
    out: values.out === undefined ? null : resolve(values.out) };
  if (args.backend && !STACK_ADAPTER_REGISTRY.ids.includes(args.backend)) {
    throw new Error(`unknown backend ${args.backend}`);
  }
  return args;
}

function run(container: string, cwd: string, command: string,
  args: readonly string[], commands: BuildCommand[]): void {
  const started = Date.now();
  const printable = redactCredentials([command, ...args].join(' '));
  try {
    const output = execFileSync('docker', ['exec', ...codingContainerAgentExecOptions(),
      '-w', cwd, container, ...codingContainerAgentCommand(command, args)],
      { encoding: 'utf8', stdio: 'pipe', maxBuffer: 64 * 1024 * 1024 });
    commands.push({ cwd, command: printable, ok: true, durationMs: Date.now() - started,
      outputTail: redactCredentials(output).slice(-8_192) });
  } catch (error) {
    const output = redactCredentials(streams(error, 'stdout', 'stderr'));
    commands.push({ cwd, command: printable, ok: false, durationMs: Date.now() - started,
      outputTail: output.slice(-16_384) });
    throw new Error(`${printable} failed in ${cwd}:\n${output.slice(-16_384)}`);
  }
}

function buildCommands(metadata: ReferenceMetadataForBuild, container: string,
  commands: BuildCommand[]): void {
  for (const directory of metadata.installDirectories) {
    run(container, `${CODING_CONTAINER_APP_ROOT}/${directory}`, 'node', ['-e',
      "const fs=require('node:fs'); for(const f of ['package.json','package-lock.json']) if(!fs.existsSync(f)) throw new Error(`${process.cwd()}/${f} is missing`);"], commands);
  }
  for (const step of referenceInstallSteps(metadata)) {
    run(container, `${CODING_CONTAINER_APP_ROOT}/${step.directory}`, step.command, step.args, commands);
  }
  if (metadata.kind === 'node-api') {
    run(container, `${CODING_CONTAINER_APP_ROOT}/${metadata.server.directory}`,
      'npm', ['exec', 'tsc', '--', '--noEmit'], commands);
  run(container, `${CODING_CONTAINER_APP_ROOT}/${metadata.client.directory}`,
    'npm', ['run', 'build'], commands);
    return;
  }
  run(container, `${CODING_CONTAINER_APP_ROOT}/${metadata.moduleDirectory}`, CODING_CONTAINER_SPACETIME_CLI,
    ['build', '--module-path', `${CODING_CONTAINER_APP_ROOT}/${metadata.moduleDirectory}`], commands);
  run(container, `${CODING_CONTAINER_APP_ROOT}/${metadata.moduleDirectory}`, CODING_CONTAINER_SPACETIME_CLI,
    ['generate', '--lang', 'typescript', '--module-path', `${CODING_CONTAINER_APP_ROOT}/${metadata.moduleDirectory}`,
      '--out-dir', `${CODING_CONTAINER_APP_ROOT}/${metadata.bindingsDirectory}`, '--yes', '--no-config'], commands);
  run(container, `${CODING_CONTAINER_APP_ROOT}/${metadata.client.directory}`,
    'npm', ['run', 'build'], commands);
}

function qualify(fixture: ReferenceFixture, imageIdentity: ImageIdentity): FixtureBuild {
  const started = Date.now();
  const runtimeRoot = process.env.STACK_BENCH_RUNTIME_DIR;
  if (process.env.STACK_BENCH_APPLIANCE === '1' && !runtimeRoot) {
    throw new Error('reference builds require the appliance STACK_BENCH_RUNTIME_DIR shared mount');
  }
  if (runtimeRoot) mkdirSync(runtimeRoot, { recursive: true, mode: 0o700 });
  const work = mkdtempSync(join(runtimeRoot ?? tmpdir(), `stack-bench-reference-${fixture.backend}-`));
  const app = join(work, 'app');
  const leasePath = join(work, ARTIFACT_FILE.backendLease);
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
    const runtime = adapter.orchestrator.config({ root: ROOT, env: process.env,
      helpers: { exists: existsSync } });
    const track = loadTrack(fixture.track);
    const ports = portsFor(track, fixture.backend, 0);
    const preparedLease = adapter.lease.prepare({
      track, runIndex: 0, runtimeDir: work,
      serverUri: runtime.lease.serverUri, env: process.env,
      helpers: {
        moduleName: () => `reference-${fixture.track}`,
        dbName: () => `reference_${fixture.track}`,
        containerIdentity: runningContainerIdentity,
      },
    });
    const preparedResources = preparedLease.lease;
    const lockKeys = preparedLease.lockKeys;
    lease = createBackendLease({ runId: basename(work), backend: fixture.backend,
      track: fixture.track, runIndex: 0,
      ...preparedResources });
    claimBackendResources(leasePath, lease, { ...resourceLockScope(),
      keys: backendResourceLockKeys(lease, ports, lockKeys) });
    adapter.lifecycle.activate({ leasePath, leaseToken: lease.ownershipToken, lease,
      ports,
      ...runtime.lifecycle });
    const prepared = execFileSync(process.execPath,
      [RUN_BUILD, '--app', app, '--backend', fixture.backend, '--image', IMAGE, '--prepare-only'],
      { encoding: 'utf8', stdio: 'pipe', maxBuffer: 64 * 1024 * 1024,
        env: { ...process.env, ...runtime.environment,
          STACK_BENCH_LEASE: leasePath, STACK_BENCH_LEASE_TOKEN: lease.ownershipToken } });
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
        'cat', `${CODING_CONTAINER_APP_ROOT}/${REFERENCE_METADATA_FILE}`],
      { encoding: 'utf8', stdio: 'pipe' }));
    const metadataIssues = referenceMetadataIssues(metadata);
    if (metadataIssues.length) throw new Error(metadataIssues.join('; '));
    buildCommands(metadata, identity.containerName, commands);
  } catch (caught) {
    error = errorDetail(caught);
  } finally {
    if (lease) {
      try {
        if (existsSync(leasePath)) {
          if (!releaseBackendLease(leasePath, lease.ownershipToken)) {
            cleanupComplete = false;
            recordError('authenticated runtime cleanup refused; resource authority retained');
          }
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
  const fixtures = registry.fixtures.filter(fixture => !args.backend || fixture.backend === args.backend)
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
    console.error(errorDetail(error));
    process.exitCode = 2;
  });
}
