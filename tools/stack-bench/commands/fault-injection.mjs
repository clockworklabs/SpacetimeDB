#!/usr/bin/env node
// Model-free lifecycle fault test for the production harness.
//
// This creates real Docker and SpacetimeDB resources, enters the real restart
// script after the owned listener is stopped but before its replacement starts,
// and proves teardown removes only the exact leased resources. It also verifies
// that the launcher refuses a same-name container absent from the lease.

import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { createServer } from 'node:http';
import { basename, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';

import { createBackendLease, writeBackendLease } from '../src/runtime/backend-lease.mjs';
import { readArtifact, readArtifactPayload } from '../src/evidence/artifacts.mjs';
import { killTree, pidsOnPort } from '../src/runtime/platform.mjs';
import { DEFAULT_BUILD_IMAGE } from '../src/composition/product-config.mjs';

import { STACK_BENCH_ROOT as ROOT } from '../src/project-paths.mjs';
const REPO = resolve(ROOT, '..', '..');
const IMAGE = process.env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE;
const CLI = process.env.SPACETIME_BIN ?? join(REPO, 'target', 'release',
  process.platform === 'win32' ? 'spacetimedb-cli.exe' : 'spacetimedb-cli');
const RUN_BUILD = join(ROOT, 'container', 'run-build.mjs');
const delay = ms => new Promise(resolveDelay => setTimeout(resolveDelay, ms));

async function freePort() {
  const server = createServer((_request, response) => response.end('foreign'));
  await new Promise((ok, fail) => server.listen(0, '127.0.0.1', ok).once('error', fail));
  const port = server.address().port;
  await new Promise(ok => server.close(ok));
  return port;
}

function inspectContainer(target) {
  try {
    const output = execFileSync('docker', ['inspect', '--format',
      '{{.Id}} {{.State.Running}}', target], { encoding: 'utf8', stdio: 'pipe' }).trim();
    const [id, running] = output.split(/\s+/, 2);
    return { id, running: running === 'true' };
  } catch { return null; }
}

function startContainer(name) {
  const id = execFileSync('docker', ['run', '-d', '--init', '--name', name,
    IMAGE, 'sleep', 'infinity'], { encoding: 'utf8', stdio: 'pipe' }).trim();
  assert.ok(id, `Docker did not return an id for ${name}`);
  return inspectContainer(id);
}

function removeExactContainer(identity) {
  if (!identity?.id) return;
  const current = inspectContainer(identity.id);
  if (!current || current.id !== identity.id) return;
  execFileSync('docker', ['rm', '-f', identity.id], { stdio: 'ignore' });
}

async function waitForExit(child, timeoutMs) {
  return new Promise((resolveExit, reject) => {
    const timeout = setTimeout(() => reject(new Error(
      `bench.mjs did not exit after injected failure within ${timeoutMs}ms`)), timeoutMs);
    child.once('exit', (code, signal) => {
      clearTimeout(timeout);
      resolveExit({ code, signal });
    });
  });
}

async function assertRefusesUnleasedCollision() {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-container-collision-'));
  const app = join(root, 'app');
  const name = `stack-bench-${basename(root)}`;
  const leasePath = join(root, 'backend-lease.json');
  let foreign = null;
  try {
    mkdirSync(app, { recursive: true });
    foreign = startContainer(name);
    const lease = createBackendLease({ runId: `collision-${process.pid}`, backend: 'spacetime',
      track: 'fault-injection', runIndex: 0, serverUri: 'http://127.0.0.1:1',
      module: `collision-${process.pid}`, dataDir: join(root, 'data') });
    lease.state = 'active';
    writeBackendLease(leasePath, lease);

    let refused = false;
    try {
      execFileSync(process.execPath,
        [RUN_BUILD, '--app', app, '--backend', 'spacetime', '--prepare-only'],
        { stdio: 'pipe', env: { ...process.env, STACK_BENCH_LEASE: leasePath,
          STACK_BENCH_LEASE_TOKEN: lease.ownershipToken } });
    } catch (error) {
      refused = error.status === 3
        && /refusing to adopt existing unleased container/.test(String(error.stderr));
    }
    assert.equal(refused, true, 'launcher did not explicitly refuse an unleased same-name container');
    assert.deepEqual(inspectContainer(foreign.id), foreign,
      'collision refusal changed or stopped the foreign container');
  } finally {
    removeExactContainer(foreign);
    rmSync(root, { recursive: true, force: true });
  }
}

async function main() {
  assert.ok(existsSync(CLI), `local SpacetimeDB CLI is missing: ${CLI}`);
  execFileSync('docker', ['image', 'inspect', IMAGE], { stdio: 'pipe' });
  await assertRefusesUnleasedCollision();

  const root = mkdtempSync(join(tmpdir(), 'stack-bench-fault-'));
  const app = join(root, 'app');
  const out = join(root, 'out');
  const markerPath = join(app, '.fault-ready.json');
  const port = await freePort();
  const uri = `http://127.0.0.1:${port}`;
  const foreignName = `stack-bench-foreign-${process.pid}-${Date.now()}`;
  let foreignContainer = null;
  let foreignServer = null;
  let bench = null;
  let marker = null;
  let output = '';

  try {
    mkdirSync(app, { recursive: true });
    mkdirSync(out, { recursive: true });
    foreignContainer = startContainer(foreignName);
    foreignServer = createServer((_request, response) => response.end('foreign'));
    await new Promise((ok, fail) => foreignServer.listen(0, '127.0.0.1', ok).once('error', fail));
    const foreignUri = `http://127.0.0.1:${foreignServer.address().port}`;

    bench = spawn(process.execPath,
      [join(ROOT, 'commands', 'bench.mjs'), '--backend', 'spacetime', '--track', 'loop',
        '--levels', '1', '--agent-adapter', 'fault-injection', '--app', app, '--out', out,
        '--url', `file:///${app.replace(/\\/g, '/')}/index.html`, '--skip-probe'],
      { env: { ...process.env, STACK_BENCH_STDB_URI: uri, STACK_BENCH_IMAGE: IMAGE },
        stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
    const collect = chunk => { output = (output + chunk.toString()).slice(-256 * 1024); };
    bench.stdout.on('data', collect);
    bench.stderr.on('data', collect);
    const exited = await waitForExit(bench, 300_000);
    assert.notEqual(exited.code, 0, 'injected coding-agent failure unexpectedly exited zero');
    assert.ok(existsSync(markerPath), `fault marker was not written before failure:\n${output}`);
    marker = JSON.parse(readFileSync(markerPath, 'utf8'));
    assert.equal(marker.phase, 'restart-stopped',
      'fault was not injected inside the backend restart window');
    assert.equal(marker.lease.state, 'restarting');
    assert.match(marker.lease.resources.buildContainer.image, /^sha256:[0-9a-f]{64}$/,
      'build container lease did not record an immutable image id');

    const evidencePath = join(out, 'backend-lease.json');
    assert.ok(existsSync(evidencePath), `teardown did not preserve lease evidence:\n${output}`);
    const evidence = readArtifactPayload(evidencePath, { expectedKind: 'backend_lease_evidence' });
    const preflight = readArtifact(join(out, 'preflight.json'), { expectedKind: 'preflight' });
    assert.equal(preflight.payload.ok, true, 'paid-run preflight did not pass');
    assert.equal(preflight.attempt.parentId, marker.lease.runId,
      'preflight evidence is not attached to the run it admitted');
    assert.equal(evidence.runId, marker.lease.runId);
    assert.equal(evidence.state, 'released', 'benchmark lease did not reach its terminal state');
    assert.ok(evidence.stoppedAt, 'benchmark-owned SpacetimeDB host has no stop evidence');
    assert.ok(evidence.releasedAt, 'benchmark lease has no release evidence');
    assert.deepEqual(evidence.resources.listenerPids, []);
    assert.equal(evidence.resources.buildContainer.running, false,
      'benchmark-owned build container was not marked removed');
    assert.ok(evidence.resources.buildContainer.removedAt);
    assert.ok(evidence.resources.locks.every(lock => lock.releasedAt),
      'one or more resource locks were not released');
    assert.equal(inspectContainer(marker.lease.resources.buildContainer.id), null,
      'benchmark-owned build container survived fatal cleanup');
    assert.equal(pidsOnPort(port).length, 0, 'benchmark-owned listener survived fatal cleanup');
    assert.equal(existsSync(marker.leasePath), false, 'private runtime lease was not removed');

    assert.equal((await fetch(foreignUri)).status, 200,
      'foreign listener was disturbed by benchmark cleanup');
    assert.deepEqual(inspectContainer(foreignContainer.id), foreignContainer,
      'foreign container was changed or removed by benchmark cleanup');

    console.log(JSON.stringify({ ok: true, injectedAt: 'restart-stopped-before-replacement',
      benchmarkHostStopped: true, benchmarkContainerRemoved: true, locksReleased: true,
      privateLeaseRemoved: true, foreignListenerSurvived: true,
      foreignContainerSurvived: true, unleasedCollisionRefused: true,
      immutableImagePinned: true }, null, 2));
  } finally {
    if (bench?.exitCode === null) {
      killTree(bench.pid);
      await delay(500);
    }
    if (marker?.lease?.resources?.buildContainer) {
      removeExactContainer(marker.lease.resources.buildContainer);
    }
    for (const pid of marker?.lease?.resources?.listenerPids ?? []) {
      if (pidsOnPort(port).includes(String(pid))) killTree(pid);
    }
    if (foreignServer) {
      // The verification fetch uses a keep-alive connection. Waiting on
      // close() alone can hold CI open until Undici retires that socket.
      foreignServer.closeAllConnections();
      await new Promise(ok => foreignServer.close(ok));
    }
    removeExactContainer(foreignContainer);
    rmSync(root, { recursive: true, force: true });
  }
}

main().catch(error => {
  console.error(error.stack ?? error.message);
  process.exitCode = 1;
});
