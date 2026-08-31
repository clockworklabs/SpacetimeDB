#!/usr/bin/env node
// Model-free production smoke for the isolated SpacetimeDB build path.
//
// Starts a dedicated host on an ephemeral loopback port and data directory,
// prepares the real build image, then builds and publishes a
// tiny TypeScript module with `spacetime dev`, verifies it through SQL, then
// removes only the container, process and data directory created here.

import { spawn, execFileSync } from 'node:child_process';
import type { ChildProcess } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { createServer } from 'node:net';
import type { AddressInfo } from 'node:net';
import { basename, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';

import { killTree, pidsOnPort, processIdentity } from '../src/runtime/platform.js';
import { createBackendLease, readBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';
import { fetchStatus } from '../src/runtime/readiness.js';
import { DEFAULT_BUILD_IMAGE } from '../src/composition/product-config.js';
import { containerReachableSpacetimeUri } from '../src/runtime/spacetime-target.js';
import { codingContainerAgentExecOptions } from '../src/runtime/coding-container-policy.js';

import { STACK_BENCH_ROOT as ROOT, compiledEntrypoint } from '../src/package-root.js';
const REPO = resolve(ROOT, '..', '..');
const IMAGE = process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE;
const CLI = process.env.SPACETIME_BIN ?? join(REPO, 'target', 'release',
  process.platform === 'win32' ? 'spacetimedb-cli.exe' : 'spacetimedb-cli');
const RUN_BUILD = compiledEntrypoint('container', 'run-build.js');
const FIXTURE = join(ROOT, 'tests', 'fixtures', 'spacetime-module');

const delay = (ms: number): Promise<void> => new Promise(resolveDelay => setTimeout(resolveDelay, ms));

interface PreparedContainerIdentity {
  containerName: string;
  identity: string;
  networkMode: string | null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function parsePreparedContainerIdentity(text: string): PreparedContainerIdentity {
  const value: unknown = JSON.parse(text.trim().split(/\r?\n/).pop() ?? '');
  if (!isRecord(value)) throw new Error('prepared container identity is invalid');
  const record = value;
  if (typeof record.containerName !== 'string' || typeof record.identity !== 'string'
    || (record.networkMode !== null && typeof record.networkMode !== 'string')) {
    throw new Error('prepared container identity is invalid');
  }
  return { containerName: record.containerName, identity: record.identity, networkMode: record.networkMode };
}

async function freePort() {
  const server = createServer();
  await new Promise<void>((ok, fail) => server.listen({ port: 0, host: '127.0.0.1' }, ok).once('error', fail));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('could not allocate a TCP port');
  const port: AddressInfo['port'] = address.port;
  await new Promise(ok => server.close(ok));
  return port;
}

async function waitFor(check: () => boolean | Promise<boolean>, timeoutMs: number, description: string): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await check()) return;
    await delay(250);
  }
  throw new Error(`timed out waiting for ${description}`);
}

async function main() {
  if (!existsSync(CLI)) throw new Error(`local SpacetimeDB CLI is missing: ${CLI}`);
  execFileSync('docker', ['image', 'inspect', IMAGE], { stdio: 'pipe' });

  const root = mkdtempSync(join(tmpdir(), 'stack-bench-container-smoke-'));
  const app = join(root, 'app');
  const dataDir = join(root, 'spacetime-data');
  const port = await freePort();
  const uri = `http://127.0.0.1:${port}`;
  const module = `stackbench-container-smoke-${process.pid}`;
  const containerName = `stack-bench-${basename(root)}`;
  const leasePath = join(root, 'backend-lease.json');
  let host: ChildProcess | null = null;
  let dev: ChildProcess | null = null;
  let output = '';

  try {
    mkdirSync(app, { recursive: true });
    host = spawn(CLI, ['start', '--listen-addr', `127.0.0.1:${port}`, '--data-dir', dataDir],
      { stdio: 'ignore', windowsHide: true });
    await waitFor(async () => {
      const status = await fetchStatus(`${uri}/v1/ping`, { timeoutMs: 5000 });
      return status !== null && status >= 200 && status < 300;
    }, 120_000, `dedicated SpacetimeDB host on :${port}`);

    const lease = createBackendLease({ runId: basename(root), backend: 'spacetime',
      track: 'container-smoke', runIndex: 0, serverUri: uri, module, dataDir });
    lease.state = 'active';
    lease.resources.launchedProcess = host.pid ? processIdentity(host.pid) : null;
    lease.resources.listenerProcesses = pidsOnPort(port).map(pid => processIdentity(pid))
      .filter((identity): identity is NonNullable<typeof identity> => identity !== null);
    writeBackendLease(leasePath, lease);

    const prepared = execFileSync(process.execPath,
      [RUN_BUILD, '--app', app, '--backend', 'spacetime', '--image', IMAGE, '--prepare-only'],
      { encoding: 'utf8', stdio: 'pipe', maxBuffer: 16 * 1024 * 1024,
        env: { ...process.env, STACK_BENCH_LEASE: leasePath,
          STACK_BENCH_LEASE_TOKEN: lease.ownershipToken } });
    const identity = parsePreparedContainerIdentity(prepared);
    if (identity.containerName !== containerName) {
      throw new Error(`prepared unexpected container ${identity.containerName}`);
    }
    const leasedContainer = readBackendLease(leasePath,
      { token: lease.ownershipToken, backend: 'spacetime', active: true }).resources.buildContainer;
    if (!leasedContainer || identity.identity.split(' ')[0] !== leasedContainer.id) {
      throw new Error('prepared container identity was not recorded in the backend lease');
    }
    if (!leasedContainer.image || !/^sha256:[0-9a-f]{64}$/.test(leasedContainer.image)) {
      throw new Error(`prepared container did not record an immutable image id: ${leasedContainer.image}`);
    }

    cpSync(FIXTURE, join(app, 'spacetimedb'), { recursive: true });
    const agentExec = ['exec', ...codingContainerAgentExecOptions()];
    execFileSync('docker', [...agentExec, containerName, 'sh', '-c',
      'umask 000; cd /app/spacetimedb && npm install --no-audit --no-fund'], { stdio: 'pipe' });

    const startedDev = spawn('docker', [...agentExec, '-i', containerName, 'sh', '-c',
      `umask 000; cd /app/spacetimedb && /deps/spacetimedb-cli dev ${module} `
      + '--no-config --project-path /app/spacetimedb --module-path . '
      + '--server-only --skip-generate '
      + `-s ${containerReachableSpacetimeUri({ resources: { serverUri: uri,
        buildContainer: lease.resources.buildContainer } }, identity.networkMode)} -y`],
    { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
    dev = startedDev;
    const collect = (chunk: Buffer): void => { output = (output + chunk.toString()).slice(-128 * 1024); };
    startedDev.stdout?.on('data', collect);
    startedDev.stderr?.on('data', collect);

    await waitFor(() => {
      if (/Published successfully!/.test(output)) return true;
      if (startedDev.exitCode !== null) throw new Error(`spacetime dev exited ${startedDev.exitCode}:\n${output}`);
      return false;
    }, 240_000, 'containerized module publish');

    const sql = execFileSync(CLI, ['sql', module, 'SELECT * FROM smoke_item', '-s', uri],
      { encoding: 'utf8', stdio: 'pipe' });
    if (!/\bid\s*\|\s*value\b/.test(sql)) throw new Error(`SQL verification failed:\n${sql}`);
    if (startedDev.exitCode !== null) throw new Error('spacetime dev did not remain alive as a watcher');

    // Publishing and log streaming must retain one authenticated identity. A
    // prior dev bug published with a token stored only in a Config clone, then
    // directly logged in again for logs and received an authorization error.
    await delay(2_000);
    const logStreamingAuthorized = !/Log streaming error:.*not authorized/s.test(output);
    console.log(JSON.stringify({ ok: true, image: IMAGE, container: identity.identity,
      host: { uri, listenerPids: pidsOnPort(port) }, published: true, sqlVerified: true,
      watcherAlive: true, leasedContainer: true, immutableImagePinned: true,
      logStreamingAuthorized }, null, 2));
    if (!logStreamingAuthorized) {
      throw new Error('`spacetime dev` published successfully but its log stream was not authorized');
    }
    // The grader resets by republishing the same named database from this exact
    // leased container. Prove that `-y` retained a reusable local identity,
    // rather than merely proving that the first anonymous-looking publish ran.
    execFileSync('docker', [...agentExec, containerName, 'sh', '-c',
      'for process in /proc/[0-9]*; do '
      + 'test "$(cat "$process/comm" 2>/dev/null)" = spacetimedb-cli '
      + '&& kill -TERM "${process##*/}" || true; done'], { stdio: 'pipe' });
    await waitFor(() => startedDev.exitCode !== null, 15_000, 'spacetime dev to stop before reset publish');
    const targetUri = containerReachableSpacetimeUri({ resources: { serverUri: uri,
      buildContainer: lease.resources.buildContainer } }, identity.networkMode);
    execFileSync('docker', [...agentExec, containerName, 'sh', '-c',
      `umask 000; cd /app/spacetimedb && /deps/spacetimedb-cli publish ${module} `
      + `--no-config --module-path . -s ${targetUri} --delete-data -y`],
    { stdio: 'pipe', timeout: 240_000 });
    const afterReset = execFileSync(CLI, ['sql', module, 'SELECT * FROM smoke_item', '-s', uri],
      { encoding: 'utf8', stdio: 'pipe' });
    if (!/\bid\s*\|\s*value\b/.test(afterReset)) {
      throw new Error(`SQL verification after reset publish failed:\n${afterReset}`);
    }
    console.log(JSON.stringify({ resetRepublished: true, resetSqlVerified: true }));
  } finally {
    if (dev && dev.exitCode === null) dev.kill('SIGTERM');
    try { execFileSync('docker', ['rm', '-f', containerName], { stdio: 'ignore' }); } catch { /* absent */ }
    // The port was proven unused before this script started the host. Kill only
    // listeners on that exact ephemeral port, then the wrapper if it remains.
    for (const pid of pidsOnPort(port)) killTree(pid);
    if (host && host.exitCode === null) killTree(host.pid);
    rmSync(root, { recursive: true, force: true });
  }
}

main().catch(error => {
  console.error(error.stack ?? error.message);
  process.exitCode = 1;
});
