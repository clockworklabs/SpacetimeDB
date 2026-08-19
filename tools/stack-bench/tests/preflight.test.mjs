import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

import { parsePreflightArgs, probeLoopbackPort, runPreflight } from '../src/runtime/preflight.mjs';
import { createArtifact, validateArtifact } from '../src/evidence/artifacts.mjs';
import { AGENT_ADAPTER_REGISTRY } from '../src/agents/agent-adapters.mjs';

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
    let smokeArgs;
    const run = (_file, args) => {
      if (args[0] === 'info') return dockerInfo();
      if (args[0] === 'compose') return '2.40.0';
      if (args[0] === 'image') return args[3] === '{{.Os}}/{{.Architecture}}'
        ? 'linux/amd64' : `${IMAGE_ID}\n`;
      if (args[0] === 'run') {
        smokeArgs = args;
        const mount = args[args.indexOf('-v') + 1].split(':/results')[0];
        const marker = args.at(-1);
        writeFileSync(join(mount, marker), 'container-write-ok');
        return JSON.stringify({ platform: 'linux', arch: 'x64', node: 'v22.0.0',
          reached: [{ url: 'https://registry.npmjs.org', status: 200 }],
          tcpReached: [],
          executables: {}, credentialStatus: 'not-checked',
          diskFreeBytes: 20 * 1024 ** 3 });
      }
      throw new Error(`unexpected docker command: ${args.join(' ')}`);
    };
    const report = runPreflight(request(root, ['--smoke']), {
      run, now: Date.parse('2026-08-12T12:00:00.100Z'), env: {}, home: root,
      statfs: () => ({ bavail: 20n, bsize: 1024n ** 3n }),
      pidsOnPort: () => [], probePort: () => ({ free: true }),
    });
    assert.equal(report.ok, true, JSON.stringify(report.checks, null, 2));
    assert.equal(report.request.smoke, true);
    assert.equal(report.checks.find(check => check.id === 'smoke.container').status, 'pass');
    assert.match(report.checks.find(check => check.id === 'smoke.container').summary,
      /0 database route\(s\); 0 agent executable\(s\)/);
    assert.equal(report.checks.find(check => check.id === 'storage.container').status, 'pass');
    assert.equal(report.checks.some(check => check.id === 'outbound.container'), false);
    assert.equal(smokeArgs[smokeArgs.indexOf('--network') + 1], 'bridge');
    assert.equal(smokeArgs[smokeArgs.indexOf('--add-host') + 1],
      'host.docker.internal:host-gateway');
    const artifact = createArtifact({ kind: 'preflight', id: 'preflight-test', payload: report });
    assert.equal(validateArtifact(artifact).payload.ok, true);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('preflight fails before a paid run when the selected agent executable is absent', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-agent-'));
  try {
    const selected = parsePreflightArgs(['node', 'preflight.mjs', '--backend', 'stub',
      '--track', 'loop', '--levels', '1', '--agent-adapter', 'claude-code',
      '--results-dir', root, '--smoke']);
    let requestedExecutables;
    const run = (_file, args) => {
      if (args[0] === 'info') return dockerInfo();
      if (args[0] === 'compose') return '2.40.0';
      if (args[0] === 'image') return args[3] === '{{.Os}}/{{.Architecture}}'
        ? 'linux/amd64' : `${IMAGE_ID}\n`;
      if (args[0] === 'run') {
        const mount = args[args.indexOf('-v') + 1].split(':/results')[0];
        writeFileSync(join(mount, args.at(-1)), 'container-write-ok');
        requestedExecutables = JSON.parse(args.at(-4));
        return JSON.stringify({ platform: 'linux', arch: 'x64', node: 'v22.0.0',
          reached: [], tcpReached: [], executables: {}, credentialStatus: 'not-checked',
          diskFreeBytes: 20 * 1024 ** 3 });
      }
      throw new Error(`unexpected docker command: ${args.join(' ')}`);
    };
    const report = runPreflight(selected, { run, now: Date.parse('2026-08-12T12:00:00.100Z'),
      env: { ANTHROPIC_API_KEY: '<test-present>' }, home: root,
      statfs: () => ({ bavail: 20n, bsize: 1024n ** 3n }), pidsOnPort: () => [],
      probePort: () => ({ free: true }) });
    assert.deepEqual(requestedExecutables, ['claude']);
    assert.equal(report.ok, false);
    assert.equal(report.checks.find(check => check.id === 'smoke.container').status, 'fail');
    assert.doesNotMatch(JSON.stringify(report), /test-present/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a rotating interactive credential cannot satisfy preflight', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-credential-'));
  try {
    const credential = join(root, '.claude', '.credentials.json');
    mkdirSync(join(root, '.claude'));
    writeFileSync(credential, '{}');
    const selected = parsePreflightArgs(['node', 'preflight.mjs', '--backend', 'stub',
      '--track', 'loop', '--levels', '1', '--agent-adapter', 'claude-code',
      '--results-dir', root]);
    const run = (_file, args) => {
      if (args[0] === 'info') return dockerInfo();
      if (args[0] === 'compose') return '2.40.0';
      if (args[0] === 'image') return args[3] === '{{.Os}}/{{.Architecture}}'
        ? 'linux/amd64' : `${IMAGE_ID}\n`;
      throw new Error(`unexpected docker command: ${args.join(' ')}`);
    };
    const report = runPreflight(selected, { run, now: Date.parse('2026-08-12T12:00:00.100Z'),
      env: {}, home: root, statfs: () => ({ bavail: 20n, bsize: 1024n ** 3n }),
      pidsOnPort: () => [], probePort: () => ({ free: true }) });
    assert.equal(report.ok, false);
    assert.equal(report.checks.find(check => check.id === 'agent.credentials').status, 'fail');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('long-lived subscription token is mounted read-only and checked without exposing its value', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-subscription-token-'));
  try {
    const tokenFile = join(root, 'claude-token');
    writeFileSync(tokenFile, 'must-not-appear-in-report\n');
    const selected = parsePreflightArgs(['node', 'preflight.mjs', '--backend', 'stub',
      '--track', 'loop', '--levels', '1', '--agent-adapter', 'claude-code',
      '--results-dir', root, '--smoke']);
    let dockerArgs;
    const run = (_file, args) => {
      if (args[0] === 'info') return dockerInfo();
      if (args[0] === 'compose') return '2.40.0';
      if (args[0] === 'image') return args[3] === '{{.Os}}/{{.Architecture}}'
        ? 'linux/amd64' : `${IMAGE_ID}\n`;
      if (args[0] === 'run') {
        dockerArgs = args;
        const mount = args[args.indexOf('-v') + 1].split(':/results')[0];
        writeFileSync(join(mount, args.at(-1)), 'container-write-ok');
        return JSON.stringify({ platform: 'linux', arch: 'x64', node: 'v22.0.0', reached: [],
          tcpReached: [], executables: { claude: '/usr/local/bin/claude' },
          credentialStatus: 'ready', diskFreeBytes: 20 * 1024 ** 3 });
      }
      throw new Error(`unexpected docker command: ${args.join(' ')}`);
    };
    const report = runPreflight(selected, { run, now: Date.parse('2026-08-12T12:00:00.100Z'),
      env: { CLAUDE_CODE_OAUTH_TOKEN_FILE: tokenFile }, home: root,
      statfs: () => ({ bavail: 20n, bsize: 1024n ** 3n }), pidsOnPort: () => [],
      probePort: () => ({ free: true }) });
    assert.equal(report.ok, true, JSON.stringify(report.checks, null, 2));
    assert.equal(report.checks.find(check => check.id === 'agent.authentication').status, 'pass');
    assert.match(dockerArgs[dockerArgs.indexOf('--mount') + 1],
      /src=.*claude-token,dst=\/run\/secrets\/agent-credential,readonly$/);
    assert.deepEqual(JSON.parse(dockerArgs.at(-2)), {
      name: 'CLAUDE_CODE_OAUTH_TOKEN', file: '/run/secrets/agent-credential',
    });
    assert.doesNotMatch(JSON.stringify({ report, dockerArgs }), /must-not-appear-in-report/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('preflight fails before launch when a stack default skill document is absent', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-materials-'));
  try {
    const selected = parsePreflightArgs(['node', 'preflight.mjs', '--backend', 'spacetime',
      '--track', 'ecommerce', '--levels', '1', '--agent-adapter', 'claude-code',
      '--results-dir', root]);
    const report = runPreflight(selected, {
      run: () => { throw new Error('Docker unavailable in this focused test'); },
      env: { ANTHROPIC_API_KEY: '<test-present>' }, home: root,
      exists: path => !String(path).includes(`${join('skills', 'typescript-server')}`),
      statfs: () => ({ bavail: 20n, bsize: 1024n ** 3n }), pidsOnPort: () => [],
      probePort: () => ({ free: true }),
    });
    const materials = report.checks.find(check => check.id === 'materials.spacetime.skills');
    assert.equal(materials.status, 'fail');
    assert.deepEqual(materials.evidence.skills, ['typescript-server', 'typescript-client']);
    assert.equal(materials.evidence.missing.length, 1);
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
  assert.equal(parsePreflightArgs(['node', 'preflight.mjs', '--backend', 'postgres',
    '--track', 'ecommerce', '--levels', '1', '--recipe', 'ecommerce.l1-standard@1.1.0'])
    .recipe, 'ecommerce.l1-standard@1.1.0');
  assert.throws(() => parsePreflightArgs(['node', 'preflight.mjs', '--backend', 'postgres',
    '--track', 'ecommerce', '--levels', '1-2', '--recipe', 'ecommerce.l1-standard@1.1.0']),
  /exactly one/);
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
