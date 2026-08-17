#!/usr/bin/env node
// Reproducible, model-free compile qualification for canonical fixtures.
//
// Each fixture is copied to a unique temporary work directory, built in the
// same pinned image and authenticated lease boundary used by benchmark runs,
// and removed afterwards. Canonical source is therefore never populated with
// node_modules, generated bindings or dist output.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { basename, dirname, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { writeRunJson } from './artifacts.mjs';
import { acquireResourceLock, createBackendLease, publicBackendLease,
  readBackendLease, releaseResourceLocks, updateBackendLease, writeBackendLease } from './backend-lease.mjs';
import { resolveContainerImage } from './container-image.mjs';
import { executeStackCapability } from './stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.mjs';
import { DEFAULT_BUILD_IMAGE } from './product-config.mjs';
import { loadTrack } from './tracks.mjs';
import { inspectImportedReference, loadReferenceRegistry,
  prepareReferenceFixtureSource, validateReferenceRegistry } from './reference-fixtures.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const RUN_BUILD = join(ROOT, 'container', 'run-build.mjs');
const IMAGE = process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE;

function parseArgs(argv) {
  const args = { backend: null, out: null };
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--backend') args.backend = argv[++i];
    else if (argv[i] === '--out') args.out = resolve(argv[++i]);
    else throw new Error(`unknown argument ${argv[i]}`);
  }
  if (args.backend && !STACK_ADAPTER_REGISTRY.ids.includes(args.backend)) {
    throw new Error(`unknown backend ${args.backend}`);
  }
  return args;
}

function containerIdentity(name) {
  const output = execFileSync('docker', ['inspect', '--format', '{{.Id}} {{.State.Running}}', name],
    { encoding: 'utf8', stdio: 'pipe' }).trim();
  const [id, running] = output.split(/\s+/);
  if (!/^[a-f0-9]{64}$/.test(id) || running !== 'true') throw new Error(`${name} is not a running Docker container`);
  return { name, id };
}

function run(container, cwd, command, args, commands) {
  const started = Date.now();
  const printable = [command, ...args].join(' ');
  try {
    const output = execFileSync('docker', ['exec', '-w', cwd, container, command, ...args],
      { encoding: 'utf8', stdio: 'pipe', maxBuffer: 64 * 1024 * 1024 });
    commands.push({ cwd, command: printable, ok: true, durationMs: Date.now() - started,
      outputTail: output.slice(-8_192) });
  } catch (error) {
    const output = `${error.stdout ?? ''}${error.stderr ?? ''}`;
    commands.push({ cwd, command: printable, ok: false, durationMs: Date.now() - started,
      outputTail: output.slice(-16_384) });
    throw new Error(`${printable} failed in ${cwd}:\n${output.slice(-16_384)}`);
  }
}

function buildCommands(metadata, container, commands) {
  for (const directory of metadata.installDirectories) {
    run(container, `/app/${directory}`, 'node', ['-e',
      "const fs=require('node:fs'); for(const f of ['package.json','package-lock.json']) if(!fs.existsSync(f)) throw new Error(`${process.cwd()}/${f} is missing`);"], commands);
    run(container, `/app/${directory}`, 'npm', ['ci', '--no-audit', '--no-fund'], commands);
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

function removeLeasedBuildContainer(leasePath, token) {
  const lease = readBackendLease(leasePath, { token });
  const container = lease.resources.buildContainer;
  if (!container) return;
  let actual;
  try {
    actual = execFileSync('docker', ['inspect', '--format', '{{.Id}}', container.name],
      { encoding: 'utf8', stdio: 'pipe' }).trim();
  } catch (error) {
    if (/No such (object|container)/i.test(`${error.stderr ?? ''}${error.message}`)) return;
    throw error;
  }
  if (actual !== container.id) throw new Error(`refusing to remove ${container.name}: lease id does not match`);
  execFileSync('docker', ['rm', '-f', container.id], { stdio: 'pipe' });
}

function qualify(fixture, imageIdentity) {
  const started = Date.now();
  const work = mkdtempSync(join(tmpdir(), `stack-bench-reference-${fixture.backend}-`));
  const app = join(work, 'app');
  const leasePath = join(work, 'backend-lease.json');
  const commands = [];
  let lease;
  let leaseEvidence = null;
  let error = null;
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
    lease = createBackendLease({ runId: basename(work), backend: fixture.backend,
      track: fixture.track, runIndex: 0,
      ...preparedLease.lease });
    lease.resources.locks.push(acquireResourceLock({ root: join(tmpdir(), 'stack-bench-resource-locks'),
      key: `reference-build:${fixture.id}`, lease }));
    lease.state = 'active';
    writeBackendLease(leasePath, lease);
    const prepared = execFileSync(process.execPath,
      [RUN_BUILD, '--app', app, '--backend', fixture.backend, '--image', IMAGE, '--prepare-only'],
      { encoding: 'utf8', stdio: 'pipe', maxBuffer: 64 * 1024 * 1024,
        env: { ...process.env, STACK_BENCH_LEASE: leasePath, STACK_BENCH_LEASE_TOKEN: lease.ownershipToken } });
    const identity = JSON.parse(prepared.trim().split(/\r?\n/).pop());
    const active = readBackendLease(leasePath, { token: lease.ownershipToken, backend: fixture.backend, active: true });
    if (active.resources.buildContainer?.id !== identity.identity.split(' ')[0]) {
      throw new Error('prepared container identity was not recorded in the lease');
    }
    const metadata = JSON.parse(execFileSync('docker', ['exec', identity.containerName,
      'cat', '/app/reference.json'], { encoding: 'utf8', stdio: 'pipe' }));
    buildCommands(metadata, identity.containerName, commands);
  } catch (caught) {
    error = caught.stack ?? caught.message;
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
      catch (cleanupError) { error ??= `cleanup failed: ${cleanupError.stack ?? cleanupError.message}`; }
      try {
        const finalLease = existsSync(leasePath)
          ? readBackendLease(leasePath, { token: lease.ownershipToken })
          : lease;
        releaseResourceLocks(finalLease);
        for (const lock of finalLease.resources.locks) lock.releasedAt = new Date().toISOString();
        leaseEvidence = publicBackendLease(finalLease);
      }
      catch (cleanupError) { error ??= `lock cleanup failed: ${cleanupError.stack ?? cleanupError.message}`; }
    }
    rmSync(work, { recursive: true, force: true });
  }
  return { id: fixture.id, backend: fixture.backend, ok: error === null,
    fixtureSha256: fixture.imported.sourceSha256, image: imageIdentity,
    durationMs: Date.now() - started, commands, backendLease: leaseEvidence, error };
}

async function main() {
  const args = parseArgs(process.argv);
  const registry = loadReferenceRegistry();
  const validation = validateReferenceRegistry(registry);
  if (!validation.ok) throw new Error(`reference registry is invalid:\n${validation.issues.join('\n')}`);
  const fixtures = registry.fixtures.filter(fixture => fixture.status !== 'blocked')
    .filter(fixture => !args.backend || fixture.backend === args.backend);
  if (!fixtures.length) throw new Error('no imported fixtures matched');
  for (const fixture of fixtures) {
    const inspection = inspectImportedReference(fixture);
    if (!inspection.ok) throw new Error(`${fixture.id} import is invalid:\n${inspection.failures.join('\n')}`);
  }
  const imageIdentity = resolveContainerImage(IMAGE);
  const id = `reference-build-${new Date().toISOString().replace(/[-:.TZ]/g, '').slice(0, 14)}-${process.pid}`;
  const artifact = { id, kind: 'reference_build', startedAt: new Date().toISOString(),
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
  main().catch(error => { console.error(error.stack ?? error.message); process.exitCode = 2; });
}
