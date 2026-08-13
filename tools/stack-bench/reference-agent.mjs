#!/usr/bin/env node
// Model-free deploy adapter for live reference qualification.
//
// bench.mjs owns the backend lease, grader, resets, restart tests, artifacts
// and teardown. This adapter replaces only the coding session: it installs and
// deploys the already-selected fixture in the leased build container.

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { leaseFromEnv } from './backend-lease.mjs';
import { dbName, loadTrack, moduleName, portsFor } from './tracks.mjs';
import { fetchStatus } from './readiness.mjs';
import { executeStackCapability } from './stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from './stack-adapters.mjs';
import { DEFAULT_BUILD_IMAGE } from './product-config.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const RUN_BUILD = join(ROOT, 'container', 'run-build.mjs');
const IMAGE = process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE;
const COMMAND_TIMEOUT_MS = 15 * 60_000;
const delay = ms => new Promise(resolveDelay => setTimeout(resolveDelay, ms));
const phase = message => process.stderr.write(`[reference-agent] ${message}\n`);
function installProcessHandlers() {
  process.on('beforeExit', code => phase(`beforeExit code=${code}`));
  process.on('exit', code => phase(`exit code=${code}`));
  for (const signal of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
    process.on(signal, () => {
      phase(`received ${signal}`);
      process.exit(128 + { SIGINT: 2, SIGTERM: 15, SIGHUP: 1 }[signal]);
    });
  }
}

export function parseReferenceAgentArgs(argv) {
  const args = {};
  for (let i = 2; i < argv.length; i += 2) args[argv[i].replace(/^--/, '')] = argv[i + 1];
  if (!args.backend || !args.app || !args.track || !args.level || !args['run-index']) {
    throw new Error('reference-agent requires backend, app, track, level and run-index');
  }
  args.level = Number(args.level);
  args.runIndex = Number(args['run-index']);
  if (args.mode !== 'build') throw new Error(`reference-agent supports only build mode, got ${args.mode}`);
  if (!Number.isSafeInteger(args.level) || args.level < 1) {
    throw new Error(`reference-agent requires a positive integer level, got ${args.level}`);
  }
  if (!Number.isSafeInteger(args.runIndex) || args.runIndex < 0) {
    throw new Error(`reference-agent requires a non-negative integer run-index, got ${args.runIndex}`);
  }
  return args;
}

function runSync(label, command, args, options = {}) {
  try {
    return execFileSync(command, args, { timeout: COMMAND_TIMEOUT_MS, ...options });
  } catch (error) {
    const stdout = `${error.stdout ?? ''}`.trim().slice(-2000) || '<empty>';
    const stderr = `${error.stderr ?? ''}`.trim().slice(-4000) || '<empty>';
    const summary = `${error.message ?? error}`.split(/\r?\n/)[0];
    throw new Error(`${label} failed: ${summary}\nstdout tail:\n${stdout}\nstderr tail:\n${stderr}`);
  }
}

function docker(container, cwd, command, commandArgs = [], env = {}) {
  const args = ['exec', '-w', cwd];
  for (const [name, value] of Object.entries(env)) args.push('-e', `${name}=${value}`);
  args.push(container, command, ...commandArgs);
  return runSync(`docker exec ${command}`, 'docker', args,
    { encoding: 'utf8', stdio: 'pipe', maxBuffer: 64 * 1024 * 1024 });
}

function startDetached(container, cwd, logName, env = {}) {
  const args = ['exec', '-d', '-w', cwd];
  for (const [name, value] of Object.entries(env)) args.push('-e', `${name}=${value}`);
  args.push(container, 'sh', '-lc', `exec npm run dev > /tmp/${logName}.log 2>&1`);
  runSync('starting detached reference service', 'docker', args, { stdio: 'pipe' });
}

async function waitFor(url, timeoutMs, description, diagnostics) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const status = await fetchStatus(url, { timeoutMs: 5000 });
    if (status !== null && status >= 200 && status < 300) return;
    await delay(500);
  }
  throw new Error(`timed out waiting for ${description}\n${diagnostics()}`);
}

function containerLogs(container, ...names) {
  return names.map(name => {
    try { return docker(container, '/app', 'sh', ['-lc', `tail -80 /tmp/${name}.log 2>/dev/null || true`]); }
    catch { return ''; }
  }).join('\n');
}

async function main() {
  const started = Date.now();
  phase('starting');
  const args = parseReferenceAgentArgs(process.argv);
  const adapter = STACK_ADAPTER_REGISTRY.get(args.backend);
  const track = loadTrack(args.track);
  const ports = portsFor(track, args.backend, args.runIndex);
  const { lease } = leaseFromEnv(process.env, { backend: args.backend, active: true });
  phase(`validated ${args.backend} lease ${lease.runId}`);
  if (lease.track !== args.track || lease.runIndex !== args.runIndex) throw new Error('lease does not match this reference run');
  const metadataPath = join(args.app, 'reference.json');
  if (!existsSync(metadataPath)) throw new Error(`missing ${metadataPath}`);
  const metadata = JSON.parse(readFileSync(metadataPath, 'utf8'));
  writeFileSync(resolve(args.app, '..', '.stack-bench-isolation'), 'container');
  writeFileSync(join(args.app, '.stack-bench-backend'), args.backend);

  const prepared = runSync('preparing isolated build container', process.execPath,
    [RUN_BUILD, '--app', args.app, '--backend', args.backend, '--image', IMAGE,
      '--ports', [ports.vite, ports.express].filter(Boolean).join(','), '--prepare-only'],
    { encoding: 'utf8', stdio: 'pipe', maxBuffer: 64 * 1024 * 1024, env: process.env });
  const identity = JSON.parse(prepared.trim().split(/\r?\n/).pop());
  phase(`prepared build container ${identity.containerName}`);
  await executeStackCapability(adapter, 'reference', 'deploy', {
    args, metadata, lease, track, container: identity.containerName, ports,
    buildNetworkMode: identity.networkMode,
    helpers: { dbName, loadTrack, moduleName, runSync, docker, startDetached, waitFor, containerLogs, phase },
  });

  phase('deployment complete');
  console.log(JSON.stringify({ appDir: args.app, mode: args.mode, level: args.level,
    track: args.track, backend: args.backend, model: 'reference-fixture',
    setup: { isolation: { mode: 'container', image: IMAGE,
      imageId: identity.identity.split(' ')[1] }, session: 'model-free-reference' },
    costUsd: 0, tokens: 0, outputTokens: 0,
    usage: { input: 0, output: 0, cacheWrite: 0, cacheRead: 0 }, turns: 0,
    promptBytes: 0, durationMs: Date.now() - started,
    sessionId: `reference-${args.backend}-${Date.now()}`, ok: true }));
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  installProcessHandlers();
  main().catch(error => { console.error(error.stack ?? error.message); process.exitCode = 1; });
}
