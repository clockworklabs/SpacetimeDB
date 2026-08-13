import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

import { parsePreflightArgs, probeLoopbackPort, runPreflight } from '../preflight.mjs';
import { createArtifact, validateArtifact } from '../artifacts.mjs';

const IMAGE_ID = `sha256:${'a'.repeat(64)}`;

function dockerInfo(overrides = {}) {
  return JSON.stringify({ OSType: 'linux', Architecture: 'x86_64', ServerVersion: '29.0.0',
    NCPU: 8, MemTotal: 16 * 1024 ** 3, SystemTime: '2026-08-12T12:00:00.000Z', ...overrides });
}

function request(root, extra = []) {
  return parsePreflightArgs(['node', 'preflight.mjs', '--backend', 'stub', '--track', 'loop',
    '--levels', '1', '--agent-adapter', 'deterministic', '--results-dir', root, ...extra]);
}

test('preflight validates exact scope and a model-free container/result-volume smoke', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-'));
  try {
    const run = (_file, args) => {
      if (args[0] === 'info') return dockerInfo();
      if (args[0] === 'compose') return '2.40.0';
      if (args[0] === 'image') return args[3] === '{{.Os}}/{{.Architecture}}'
        ? 'linux/amd64' : `${IMAGE_ID}\n`;
      if (args[0] === 'run') {
        const mount = args[args.indexOf('-v') + 1].split(':/results')[0];
        const marker = args.at(-1);
        writeFileSync(join(mount, marker), 'container-write-ok');
        return JSON.stringify({ platform: 'linux', arch: 'x64', node: 'v22.0.0',
          reached: [{ url: 'https://registry.npmjs.org', status: 200 }],
          diskFreeBytes: 20 * 1024 ** 3 });
      }
      throw new Error(`unexpected docker command: ${args.join(' ')}`);
    };
    const report = runPreflight(request(root, ['--smoke']), {
      run, now: Date.parse('2026-08-12T12:00:00.100Z'), env: {}, home: root,
      pidsOnPort: () => [], probePort: () => ({ free: true }),
    });
    assert.equal(report.ok, true, JSON.stringify(report.checks, null, 2));
    assert.equal(report.request.smoke, true);
    assert.equal(report.checks.find(check => check.id === 'smoke.container').status, 'pass');
    assert.equal(report.checks.find(check => check.id === 'storage.container').status, 'pass');
    assert.equal(report.checks.some(check => check.id === 'outbound.container'), false);
    const artifact = createArtifact({ kind: 'preflight', id: 'preflight-test', payload: report });
    assert.equal(validateArtifact(artifact).payload.ok, true);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('preflight fails closed on inherited ownership, unavailable Docker, and occupied ports', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-'));
  try {
    const report = runPreflight(request(root), {
      run: () => { throw new Error('daemon unavailable'); },
      now: Date.parse('2026-08-12T12:00:00Z'),
      env: { STACK_BENCH_LEASE_TOKEN: 'must-not-be-reported' }, home: root,
      pidsOnPort: () => ['4242'], probePort: () => ({ free: false }),
    });
    assert.equal(report.ok, false);
    assert.equal(report.checks.find(check => check.id === 'ambient.stack_bench_lease_token').status, 'fail');
    assert.equal(report.checks.find(check => check.id === 'docker.engine').status, 'fail');
    assert.equal(report.checks.find(check => check.id === 'port.stub.vite').status, 'fail');
    assert.doesNotMatch(JSON.stringify(report), /must-not-be-reported/);
    assert.equal(report.checks.find(check => check.id === 'outbound.container').status, 'warn');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a supervised benchmark accepts only its exact fresh handoff path', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-supervisor-'));
  try {
    const supervisorState = join(root, 'supervisor.json');
    const dependencies = { run: () => { throw new Error('daemon unavailable'); },
      now: Date.parse('2026-08-12T12:00:00Z'), home: root, pidsOnPort: () => [],
      probePort: () => ({ free: true }) };
    const accepted = runPreflight({ ...request(root), supervisorState }, {
      ...dependencies, env: { STACK_BENCH_SUPERVISOR_STATE: supervisorState },
    });
    assert.equal(accepted.checks.find(check =>
      check.id === 'ambient.stack_bench_supervisor_state').status, 'pass');

    writeFileSync(supervisorState, '{}');
    const stale = runPreflight({ ...request(root), supervisorState }, {
      ...dependencies, env: { STACK_BENCH_SUPERVISOR_STATE: supervisorState },
    });
    assert.equal(stale.checks.find(check =>
      check.id === 'ambient.stack_bench_supervisor_state').status, 'fail');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('preflight argument parsing rejects ambiguous ranges and missing backends', () => {
  assert.throws(() => parsePreflightArgs(['node', 'preflight.mjs']), /--backend is required/);
  assert.throws(() => parsePreflightArgs(['node', 'preflight.mjs', '--backend', 'stub',
    '--levels', '3-1']), /--levels/);
  assert.throws(() => parsePreflightArgs(['node', 'preflight.mjs', '--backend', 'stub',
    '--mystery']), /unknown argument/);
});

test('appliance preflight defaults to its configured persistent result directory', () => {
  const resultsDir = resolve(tmpdir(), 'stack-bench-appliance-results');
  const parsed = parsePreflightArgs(['node', 'preflight.mjs', '--backend', 'postgres'], {
    env: { STACK_BENCH_RESULTS_DIR: resultsDir, STACK_BENCH_IMAGE: 'exact-build-image' },
  });
  assert.equal(parsed.resultsDir, resultsDir);
  assert.equal(parsed.image, 'exact-build-image');
});

test('unknown requested scope becomes a failed report instead of terminating the process', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-'));
  try {
    const report = runPreflight({ ...request(root), track: 'not-a-track' }, {
      run: (_file, args) => args[0] === 'info' ? dockerInfo()
        : args[0] === 'compose' ? '2.40.0'
          : args[3] === '{{.Os}}/{{.Architecture}}' ? 'linux/amd64' : IMAGE_ID,
      now: Date.parse('2026-08-12T12:00:00Z'), env: {}, home: root, pidsOnPort: () => [],
      probePort: () => ({ free: true }),
    });
    assert.equal(report.ok, false);
    assert.equal(report.checks.find(check => check.id === 'request.scope').status, 'fail');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('loopback port proof detects a listener without relying on process visibility', async t => {
  const { createServer } = await import('node:net');
  const server = createServer();
  await new Promise((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolveListen);
  });
  t.after(() => server.close());
  assert.deepEqual(probeLoopbackPort(server.address().port), { free: false });
});
