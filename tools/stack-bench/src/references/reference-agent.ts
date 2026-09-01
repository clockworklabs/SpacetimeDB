#!/usr/bin/env node
// Replace only the coding session with a selected reference fixture.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { leaseFromEnv } from '../runtime/backend-lease.js';
import { dbName, loadTrack, moduleName, portsFor } from '../composition/tracks.js';
import { fetchStatus } from '../runtime/readiness.js';
import { CODING_CONTAINER_AGENT, CODING_CONTAINER_CONTROL_DIR,
  codingContainerAgentCommand, codingContainerAgentExecOptions }
  from '../runtime/coding-container-policy.js';
import { executeStackCapability } from '../stacks/stack-adapter-contract.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';
import { DEFAULT_BUILD_IMAGE } from '../composition/product-config.js';
import { inspectImportedReference, loadReferenceRegistry, prepareReferenceFixtureSource,
  REFERENCE_METADATA_FILE, referenceMetadataIssues, validateReferenceRegistry }
  from './reference-fixtures.js';
import { resolveReferenceSelection } from './reference-selection.js';
import { assertPlainAppSourceTree, hashAppSource } from '../runtime/source-snapshot.js';

import { compiledEntrypoint } from '../package-root.js';
const RUN_BUILD = compiledEntrypoint('container', 'run-build.js');
const IMAGE = process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE;
const CONTROL_DIR = CODING_CONTAINER_CONTROL_DIR;
const COMMAND_TIMEOUT_MS = 15 * 60_000;

import type { ReferenceFixture } from './reference-fixtures.js';

// The flags this adapter is launched with, after validation.
export interface ReferenceSourceRequest {
  backend: string;
  app: string;
  track: string;
  level: number;
  mode?: string;
  recipe?: string;
}

export interface ReferenceAgentArgs extends ReferenceSourceRequest {
  runIndex: number;
}

interface PreparedReferenceSource {
  fixture: ReferenceFixture;
  seeded: boolean;
  sourceSha256: string;
  sourceFiles: number;
}

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object';

// A failed child process carries its output on the error.
const stream = (error: unknown, key: string): string =>
  record(error) ? String(error[key] ?? '') : '';

const delay = (ms: number): Promise<void> =>
  new Promise(resolveDelay => setTimeout(resolveDelay, ms));
const phase = (message: string): void => {
  process.stderr.write(`[reference-agent] ${message}\n`);
};
function installProcessHandlers(): void {
  process.on('beforeExit', code => phase(`beforeExit code=${code}`));
  process.on('exit', code => phase(`exit code=${code}`));
  for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
    process.on(signal, () => {
      phase(`received ${signal}`);
      const offsets: Record<string, number> = { SIGINT: 2, SIGTERM: 15, SIGHUP: 1 };
      process.exit(128 + (offsets[signal] ?? 0));
    });
  }
}

export function parseReferenceAgentArgs(argv: readonly string[]): ReferenceAgentArgs {
  const { values } = parseArgs({ args: [...argv.slice(2)], options: {
    mode: { type: 'string' }, backend: { type: 'string' }, level: { type: 'string' },
    app: { type: 'string' }, track: { type: 'string' }, 'run-index': { type: 'string' },
    model: { type: 'string' }, guidance: { type: 'string' }, recipe: { type: 'string' },
    'guidance-document-json': { type: 'string' }, 'credential-aliases-json': { type: 'string' },
    'skill-identity-json': { type: 'string' }, 'skills-json': { type: 'string' },
    'recipe-task-json': { type: 'string' }, 'pricing-json': { type: 'string' },
    'max-budget-usd': { type: 'string' },
  } });
  const { backend, app, track, mode } = values;
  if (typeof backend !== 'string' || typeof app !== 'string' || typeof track !== 'string'
    || values.level === undefined || values['run-index'] === undefined) {
    throw new Error('reference-agent requires backend, app, track, level and run-index');
  }
  const level = Number(values.level);
  const runIndex = Number(values['run-index']);
  if (typeof mode !== 'string' || !['build', 'upgrade', 'fix'].includes(mode)) {
    throw new Error(`reference-agent supports only build, upgrade and fix modes, got ${mode}`);
  }
  if (!Number.isSafeInteger(level) || level < 1) {
    throw new Error(`reference-agent requires a positive integer level, got ${level}`);
  }
  if (!Number.isSafeInteger(runIndex) || runIndex < 0) {
    throw new Error(`reference-agent requires a non-negative integer run-index, got ${runIndex}`);
  }
  return { backend, app, track, mode, level, runIndex,
    ...(values.recipe === undefined ? {} : { recipe: values.recipe }) };
}

function runSync(label: string, command: string, args: readonly string[],
  options: Record<string, unknown> = {}): string {
  try {
    return String(execFileSync(command, args, { timeout: COMMAND_TIMEOUT_MS, ...options }));
  } catch (error) {
    const stdout = stream(error, 'stdout').trim().slice(-2000) || '<empty>';
    const stderr = stream(error, 'stderr').trim().slice(-4000) || '<empty>';
    const summary = `${stream(error, 'message') || String(error)}`.split(/\r?\n/)[0];
    throw new Error(`${label} failed: ${summary}\nstdout tail:\n${stdout}\nstderr tail:\n${stderr}`);
  }
}

function docker(container: string, cwd: string, command: string,
  commandArgs: readonly string[] = [], env: Record<string, string> = {}): string {
  const args = ['exec', ...codingContainerAgentExecOptions(), '-w', cwd];
  for (const [name, value] of Object.entries(env)) args.push('-e', `${name}=${value}`);
  args.push(container, ...codingContainerAgentCommand(command, commandArgs));
  return runSync(`docker exec ${command}`, 'docker', args,
    { encoding: 'utf8', stdio: 'pipe', maxBuffer: 64 * 1024 * 1024 });
}

export function referenceDevCommand(logName: string,
  { networkVisible = false, port = null, script = 'dev' }: {
    networkVisible?: boolean; port?: number | null; script?: string;
  } = {}): string {
  if (!/^[a-z0-9-]+$/.test(logName)) throw new Error(`unsafe reference log name ${logName}`);
  if (!/^[a-z0-9:-]+$/.test(script)) throw new Error(`unsafe reference script ${script}`);
  if (port !== null && (!Number.isInteger(port) || port < 1 || port > 65535)) {
    throw new Error(`invalid reference port ${port}`);
  }
  const cliArgs = [networkVisible ? '--host 0.0.0.0' : null,
    port === null ? null : `--port ${port} --strictPort`].filter(Boolean);
  const networkArgs = cliArgs.length > 0 ? ` -- ${cliArgs.join(' ')}` : '';
  const agent = CODING_CONTAINER_AGENT;
  return `umask 022; exec /usr/bin/setpriv --reuid=${agent.uid} --regid=${agent.gid} --init-groups `
    + `/usr/local/bin/npm run ${script}${networkArgs} > ${CONTROL_DIR}/${logName}.log 2>&1`;
}

export function prepareReferenceSource(args: ReferenceSourceRequest): PreparedReferenceSource {
  const registry = loadReferenceRegistry();
  const validation = validateReferenceRegistry(registry);
  if (!validation.ok) {
    throw new Error(`reference registry is invalid: ${validation.issues.join('; ')}`);
  }
  const fixture = resolveReferenceSelection(registry, args).fixture;
  const inspection = inspectImportedReference(fixture);
  if (!inspection.ok) {
    throw new Error(`${fixture.id} is invalid: ${inspection.failures.join('; ')}`);
  }
  const app = String(args.app);
  mkdirSync(app, { recursive: true });
  assertPlainAppSourceTree(app);
  const before = hashAppSource(app);
  if (before.files.length === 0) {
    if ((args.mode ?? 'build') !== 'build') {
      throw new Error(`reference ${args.mode} requires the existing ${fixture.id} source`);
    }
    prepareReferenceFixtureSource(fixture, app);
  } else if (before.sha256 !== fixture.imported?.sourceSha256) {
    throw new Error(`reference app contains source other than ${fixture.id}`);
  }
  const prepared = hashAppSource(app);
  if (prepared.sha256 !== fixture.imported?.sourceSha256) {
    throw new Error(`prepared reference source does not match ${fixture.id}`);
  }
  return { fixture, seeded: before.files.length === 0, sourceSha256: prepared.sha256,
    sourceFiles: prepared.files.length };
}

export function restoreReferenceSourceIdentity(fixture: ReferenceFixture,
  app: string): { sha256: string; files: string[] } {
  prepareReferenceFixtureSource(fixture, app);
  const restored = hashAppSource(app);
  if (restored.sha256 !== fixture.imported?.sourceSha256) {
    throw new Error(`deployed reference source does not match ${fixture.id}`);
  }
  return restored;
}

export async function deployReferenceAndRestoreSource(deploy: () => unknown,
  restore: () => unknown): Promise<void> {
  let deployError: unknown = null;
  try { await deploy(); }
  catch (error) { deployError = error; }
  try { restore(); }
  catch (restoreError) {
    if (deployError) {
      throw new AggregateError([deployError, restoreError],
        'reference deployment and source restoration both failed');
    }
    throw restoreError;
  }
  if (deployError) throw deployError;
}

function startDetached(container: string, cwd: string, logName: string,
  env: Record<string, string> = {},
  options: { networkVisible?: boolean; port?: number | null; script?: string } = {}): void {
  const args = ['exec', '-d', '-w', cwd,
    '-e', `HOME=${CODING_CONTAINER_AGENT.home}`, '-e', `USER=${CODING_CONTAINER_AGENT.name}`];
  for (const [name, value] of Object.entries(env)) args.push('-e', `${name}=${value}`);
  args.push(container, 'sh', '-c', referenceDevCommand(logName, options));
  runSync('starting detached reference service', 'docker', args, { stdio: 'pipe' });
}

async function waitFor(url: string, timeoutMs: number, description: string,
  diagnostics: () => string): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const status = await fetchStatus(url, { timeoutMs: 5000 });
    if (status !== null && status >= 200 && status < 300) return;
    await delay(500);
  }
  throw new Error(`timed out waiting for ${description}\n${diagnostics()}`);
}

function containerLogs(container: string, ...names: readonly string[]): string {
  return names.map(name => {
    try {
      return runSync('reading reference logs', 'docker', ['exec', container, 'sh', '-c',
        `tail -80 ${CONTROL_DIR}/${name}.log 2>/dev/null || true`],
      { encoding: 'utf8', stdio: 'pipe' });
    }
    catch { return ''; }
  }).join('\n');
}

async function main(): Promise<void> {
  const started = Date.now();
  phase('starting');
  const args = parseReferenceAgentArgs(process.argv);
  const source = prepareReferenceSource(args);
  phase(`${source.seeded ? 'seeded' : 'verified'} ${source.fixture.id} (${source.sourceSha256.slice(0, 12)})`);
  const adapter = STACK_ADAPTER_REGISTRY.get(args.backend);
  const track = loadTrack(args.track);
  const ports = portsFor(track, args.backend, args.runIndex);
  const { lease } = leaseFromEnv(process.env, { backend: args.backend, active: true });
  phase(`validated ${args.backend} lease ${lease.runId}`);
  if (lease.track !== args.track || lease.runIndex !== args.runIndex) throw new Error('lease does not match this reference run');
  const metadataPath = join(args.app, REFERENCE_METADATA_FILE);
  if (!existsSync(metadataPath)) throw new Error(`missing ${metadataPath}`);
  const metadata: unknown = JSON.parse(readFileSync(metadataPath, 'utf8'));
  const metadataIssues = referenceMetadataIssues(metadata);
  if (metadataIssues.length) throw new Error(metadataIssues.join('; '));
  writeFileSync(resolve(args.app, '..', '.stack-bench-isolation'), 'container');
  writeFileSync(resolve(args.app, '..', '.stack-bench-backend'), args.backend);

  const prepared = runSync('preparing isolated build container', process.execPath,
    [RUN_BUILD, '--app', args.app, '--backend', args.backend, '--image', IMAGE,
      '--ports', [ports.vite, ports.express].filter(Boolean).join(','), '--prepare-only'],
    { encoding: 'utf8', stdio: 'pipe', maxBuffer: 64 * 1024 * 1024, env: process.env });
  const identity: unknown = JSON.parse(prepared.trim().split(/\r?\n/).pop() ?? '');
  if (!record(identity) || typeof identity.containerName !== 'string'
    || typeof identity.identity !== 'string') {
    throw new Error('prepared build container did not report its name and identity');
  }
  const containerIdentity = identity.identity;
  phase(`prepared build container ${identity.containerName}`);
  await deployReferenceAndRestoreSource(() => executeStackCapability(adapter, 'reference', 'deploy', {
    args, metadata, lease, track, container: identity.containerName, ports,
    buildNetworkMode: identity.networkMode,
    helpers: { dbName, loadTrack, moduleName, runSync, docker, startDetached, waitFor, containerLogs, phase },
  }), () => restoreReferenceSourceIdentity(source.fixture, args.app));

  phase('deployment complete');
  console.log(JSON.stringify({ appDir: args.app, mode: args.mode, level: args.level,
    track: args.track, backend: args.backend, model: 'reference-fixture',
    setup: { isolation: { mode: 'container', image: IMAGE,
      imageId: containerIdentity.split(' ')[1] }, session: 'model-free-reference' },
    costUsd: 0, tokens: 0, outputTokens: 0,
    usage: { input: 0, output: 0, cacheWrite: 0, cacheRead: 0 }, turns: 0,
    promptBytes: 0, durationMs: Date.now() - started,
    sessionId: `reference-${args.backend}-${Date.now()}`, ok: true }));
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  installProcessHandlers();
  main().catch((error: unknown) => {
    console.error(error instanceof Error ? error.stack ?? error.message : String(error));
    process.exitCode = 1;
  });
}
