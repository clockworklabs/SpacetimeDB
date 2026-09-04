import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { parsePreflightArgs } from '../commands/preflight-cli.js';
import { probeLoopbackPort, runPreflight, verifyPostgresServiceIdentity }
  from '../src/runtime/preflight.js';
import type { PreflightCheck, PreflightReport } from '../src/runtime/preflight.js';
import { isExactImageReference } from '../src/runtime/container-image.js';
import { createArtifact, validateArtifact } from '../src/evidence/artifacts.js';
import { compileCampaignFile, validateCampaignDefinition }
  from '../src/campaigns/campaign-compiler.js';
import { preflightResourceFloors } from '../src/composition/product-config.js';

const IMAGE_ID = `sha256:${'a'.repeat(64)}`;
const EXACT_IMAGE = `registry.example/stack-bench/build@${IMAGE_ID}`;
const progressionCampaign = join(STACK_BENCH_ROOT, 'appliance',
  'campaign.ecommerce-progression-reference.json');
let progressionPlanCache: ReturnType<typeof compileCampaignFile> | null = null;
const progressionPlan = (): ReturnType<typeof compileCampaignFile> =>
  structuredClone(progressionPlanCache ??= compileCampaignFile(progressionCampaign));

type DockerCommand = (file: string, args: string[]) => string;

function dockerInfo(overrides: Record<string, unknown> = {}): string {
  return JSON.stringify({ OSType: 'linux', Architecture: 'x86_64', ServerVersion: '29.0.0',
    NCPU: 8, MemTotal: 16 * 1024 ** 3, SystemTime: '2026-08-12T12:00:00.000Z', ...overrides });
}

function request(root: string, extra: string[] = []) {
  return parsePreflightArgs(['node', 'preflight.js', '--backend', 'stub', '--track', 'loop',
    '--levels', '1', '--agent-adapter', 'deterministic', '--results-dir', root, ...extra]);
}

function requiredArgument(args: string[], index: number): string {
  const value = args[index];
  assert.ok(value, `missing command argument at index ${index}`);
  return value;
}

function requiredCheck(report: PreflightReport, id: string): PreflightCheck {
  const check = report.checks.find(candidate => candidate.id === id);
  assert.ok(check, `missing preflight check ${id}`);
  return check;
}

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function stringArray(value: unknown, field: string): string[] {
  assert.ok(record(value), 'expected evidence object');
  const selected = value[field];
  assert.ok(Array.isArray(selected) && selected.every(item => typeof item === 'string'),
    `expected ${field} string array`);
  return selected;
}

test('PostgreSQL preflight proves the configured role, password, and database', () => {
  let command: string[] | undefined;
  const identity = verifyPostgresServiceIdentity('stack-bench-postgres', {
    execute: (file, args) => {
      command = [file, ...args];
      return 'appuser|app\n';
    },
  });
  assert.equal(identity, 'appuser|app');
  assert.ok(command);
  assert.deepEqual(command.slice(0, 4),
    ['docker', 'exec', '-e', 'PGPASSWORD=local-app-password']);
  assert(command.includes('stack-bench-postgres'));
  assert(command.includes('127.0.0.1'));
  assert.throws(() => verifyPostgresServiceIdentity('stack-bench-postgres', {
    execute: () => 'stackbench|stackbench\n',
  }), /expected appuser\|app/);
});

test('preflight validates exact scope and a model-free container/result-volume smoke', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-'));
  try {
    let smokeArgs: string[] | undefined;
    const run: DockerCommand = (_file, args) => {
      if (args[0] === 'info') return dockerInfo();
      if (args[0] === 'compose') return '2.40.0'; if (args[0] === 'ps') return '';
      if (args[0] === 'image') return args[3] === '{{.Os}}/{{.Architecture}}'
        ? 'linux/amd64' : `${IMAGE_ID}\n`;
      if (args[0] === 'run') {
        smokeArgs = args;
        const mount = requiredArgument(args, args.indexOf('-v') + 1).split(':/results')[0];
        const marker = requiredArgument(args, args.length - 1);
        assert.ok(mount);
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
    assert.equal(requiredCheck(report, 'smoke.container').status, 'pass');
    assert.match(requiredCheck(report, 'smoke.container').summary,
      /0 database route\(s\); 0 agent executable\(s\)/);
    assert.equal(requiredCheck(report, 'storage.container').status, 'pass');
    assert.equal(report.checks.some(check => check.id === 'outbound.container'), false);
    assert.ok(smokeArgs);
    assert.equal(requiredArgument(smokeArgs, smokeArgs.indexOf('--network') + 1), 'bridge');
    assert.equal(requiredArgument(smokeArgs, smokeArgs.indexOf('--add-host') + 1),
      'host.docker.internal:host-gateway');
    const artifact = createArtifact({ kind: 'preflight', id: 'preflight-test', payload: report });
    assert.equal(validateArtifact(artifact).payload.ok, true);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('appliance preflight rejects mutable controller and build image references', () => {
  assert.equal(isExactImageReference(EXACT_IMAGE), true);
  assert.equal(isExactImageReference('stack-bench-controller:latest'), false);
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-images-'));
  try {
    const run: DockerCommand = (_file, args) => {
      if (args[0] === 'info') return dockerInfo();
      if (args[0] === 'compose') return '2.40.0'; if (args[0] === 'ps') return '';
      if (args[0] === 'image') return args[3] === '{{.Os}}/{{.Architecture}}'
        ? 'linux/amd64' : `${IMAGE_ID}\n`;
      throw new Error(`unexpected docker command: ${args.join(' ')}`);
    };
    const report = runPreflight({ ...request(root), image: 'stack-bench-build:latest' }, {
      run, now: Date.parse('2026-08-12T12:00:00.100Z'),
      env: { STACK_BENCH_APPLIANCE: '1',
        STACK_BENCH_CONTROLLER_IMAGE: 'stack-bench-controller:latest' },
      home: root, statfs: () => ({ bavail: 20n, bsize: 1024n ** 3n }),
      pidsOnPort: () => [], probePort: () => ({ free: true }),
    });
    assert.equal(requiredCheck(report, 'image.controller-reference').status,
      'fail');
    assert.equal(requiredCheck(report, 'image.build-reference').status, 'fail');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('preflight does not count Docker info latency as clock skew', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-clock-'));
  try {
    const times = [
      Date.parse('2026-08-12T12:00:00.000Z'),
      Date.parse('2026-08-12T12:00:00.100Z'),
      Date.parse('2026-08-12T12:00:10.100Z'),
    ];
    const run: DockerCommand = (_file, args) => {
      if (args[0] === 'info') return dockerInfo({ SystemTime: '2026-08-12T12:00:05.000Z' });
      if (args[0] === 'compose') return '2.40.0'; if (args[0] === 'ps') return '';
      if (args[0] === 'image') return args[3] === '{{.Os}}/{{.Architecture}}'
        ? 'linux/amd64' : `${IMAGE_ID}\n`;
      throw new Error(`unexpected docker command: ${args.join(' ')}`);
    };
    const report = runPreflight(request(root), {
      run, now: () => {
        const time = times.shift();
        if (time === undefined) {
          throw new Error('preflight requested more clock samples than expected');
        }
        return time;
      },
      env: {}, home: root,
      statfs: () => ({ bavail: 20n, bsize: 1024n ** 3n }),
      pidsOnPort: () => [], probePort: () => ({ free: true }),
    });
    const clock = requiredCheck(report, 'docker.clock');
    assert.equal(clock.status, 'pass', clock.summary);
    assert.equal(clock.summary, 'host/engine skew 0 ms');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('preflight reserves capacity for all concurrent build containers', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-parallel-capacity-'));
  try {
    const run: DockerCommand = (_file, args) => {
      if (args[0] === 'info') return dockerInfo({ NCPU: 7, MemTotal: 15 * 1024 ** 3 });
      if (args[0] === 'compose') return '2.40.0'; if (args[0] === 'ps') return '';
      if (args[0] === 'image') return args[3] === '{{.Os}}/{{.Architecture}}'
        ? 'linux/amd64' : `${IMAGE_ID}\n`;
      throw new Error(`unexpected docker command: ${args.join(' ')}`);
    };
    const report = runPreflight(request(root, ['--parallelism', '3']), {
      run, now: Date.parse('2026-08-12T12:00:00.100Z'), env: {}, home: root,
      statfs: () => ({ bavail: 20n, bsize: 1024n ** 3n }),
      pidsOnPort: () => [], probePort: () => ({ free: true }),
    });
    // The host offers less than the floors the product configuration sets for
    // three concurrent attempts, so both checks fail and name the floor.
    const floors = preflightResourceFloors(3);
    assert.equal(report.request.parallelism, 3);
    assert.equal(requiredCheck(report, 'docker.cpu').status, 'fail');
    assert.match(requiredCheck(report, 'docker.cpu').remediation ?? '',
      new RegExp(`at least ${floors.cpuCount} CPUs`));
    assert.equal(requiredCheck(report, 'docker.memory').status, 'fail');
    assert.match(requiredCheck(report, 'docker.memory').remediation ?? '',
      new RegExp(`at least ${(floors.memoryBytes / 1024 ** 3).toFixed(1)} GiB`));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('an admitted campaign smoke skips only the duplicate container run', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-admitted-'));
  try {
    const run: DockerCommand = (_file, args) => {
      if (args[0] === 'info') return dockerInfo();
      if (args[0] === 'compose') return '2.40.0'; if (args[0] === 'ps') return '';
      if (args[0] === 'image') return args[3] === '{{.Os}}/{{.Architecture}}'
        ? 'linux/amd64' : `${IMAGE_ID}\n`;
      if (args[0] === 'run') throw new Error('duplicate container smoke ran');
      throw new Error(`unexpected docker command: ${args.join(' ')}`);
    };
    const admittedSmoke = { id: 'admission-1', createdAt: '2026-08-12T12:00:00.000Z' };
    const report = runPreflight({ ...request(root), smoke: false, admittedSmoke }, {
      run, now: Date.parse('2026-08-12T12:00:00.100Z'), env: {}, home: root,
      statfs: () => ({ bavail: 20n, bsize: 1024n ** 3n }),
      pidsOnPort: () => [], probePort: () => ({ free: true }),
    });
    assert.equal(requiredCheck(report, 'smoke.admission').status, 'pass');
    assert.equal(report.checks.some(check => check.id === 'smoke.container'), false);
    assert.equal(report.checks.some(check => check.id === 'outbound.container'), false);
    assert.deepEqual(report.request.admittedSmoke, admittedSmoke);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('preflight validates campaign-owned dependency selections through the progression graph', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-progression-'));
  try {
    const plan = progressionPlan();
    const report = runPreflight({
      backends: ['mongodb'],
      track: plan.definition.track,
      levelList: plan.definition.levels,
      runIndex: 0,
      agentAdapter: 'reference-fixture',
      guidance: plan.conditions[0].guidance.mode,
      packIds: [],
      checkKeys: [],
      requestedScopes: plan.conditions.map(condition => condition.requested),
      featureCatalog: plan.featureCatalog,
      mode: plan.definition.mode,
      smoke: false,
      image: 'unavailable-in-focused-test',
      resultsDir: root,
    }, {
      run: () => { throw new Error('Docker unavailable in this focused test'); },
      env: {}, home: root,
      statfs: () => ({ bavail: 20n, bsize: 1024n ** 3n }),
      pidsOnPort: () => [], probePort: () => ({ free: true }),
    });
    const scope = requiredCheck(report, 'request.scope');
    assert.equal(scope.status, 'pass', scope.summary);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('preflight validates each sequential catalog level without cumulative scope', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-sequential-'));
  try {
    const manifest = validateCampaignDefinition(
      JSON.parse(readFileSync(progressionCampaign, 'utf8')),
      { source: progressionCampaign },
    );
    manifest.id = 'sequential-preflight-proof';
    manifest.mode = { id: 'sequential' };
    manifest.repair = { selection: 'batch', budget: { total: 0 }, order: 'declared' };
    manifest.levels = [1, 2];
    assert.ok(manifest.selection.levels, 'campaign must select progression levels');
    manifest.selection.levels = manifest.selection.levels.slice(0, 2);
    const path = join(root, 'campaign.json');
    writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`);
    const plan = compileCampaignFile(path);
    const report = runPreflight({
      backends: ['mongodb'], track: plan.definition.track,
      levelList: plan.definition.levels, runIndex: 0,
      agentAdapter: 'reference-fixture', guidance: plan.conditions[0].guidance.mode,
      packIds: [], checkKeys: [],
      requestedScopes: plan.conditions.map(condition => condition.requested),
      featureCatalog: plan.featureCatalog, mode: plan.definition.mode,
      smoke: false, image: 'unavailable-in-focused-test', resultsDir: root,
    }, {
      run: () => { throw new Error('Docker unavailable in this focused test'); },
      env: {}, home: root,
      statfs: () => ({ bavail: 20n, bsize: 1024n ** 3n }),
      pidsOnPort: () => [], probePort: () => ({ free: true }),
    });
    const scope = requiredCheck(report, 'request.scope');
    assert.equal(scope.status, 'pass', scope.summary);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('preflight fails before a paid run when the selected agent executable is absent', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-agent-'));
  try {
    const selected = parsePreflightArgs(['node', 'preflight.js', '--backend', 'stub',
      '--track', 'loop', '--levels', '1', '--agent-adapter', 'claude-code',
      '--results-dir', root, '--smoke']);
    let requestedExecutables: unknown;
    const run: DockerCommand = (_file, args) => {
      if (args[0] === 'info') return dockerInfo();
      if (args[0] === 'compose') return '2.40.0'; if (args[0] === 'ps') return '';
      if (args[0] === 'image') return args[3] === '{{.Os}}/{{.Architecture}}'
        ? 'linux/amd64' : `${IMAGE_ID}\n`;
      if (args[0] === 'run') {
        const mount = requiredArgument(args, args.indexOf('-v') + 1).split(':/results')[0];
        assert.ok(mount);
        writeFileSync(join(mount, requiredArgument(args, args.length - 1)), 'container-write-ok');
        requestedExecutables = JSON.parse(requiredArgument(args, args.length - 4));
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
    assert.equal(requiredCheck(report, 'smoke.container').status, 'fail');
    assert.doesNotMatch(JSON.stringify(report), /test-present/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('paid appliance preflight refuses a dashboard on the host network', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-dashboard-'));
  try {
    for (const scenario of [
      { name: 'running dashboard service', agentAdapter: 'claude-code', dashboard: 'dashboard-id',
        portFree: true, expected: 'fail' },
      { name: 'listening dashboard port', agentAdapter: 'claude-code', dashboard: '',
        portFree: false, expected: 'fail' },
      { name: 'no dashboard', agentAdapter: 'claude-code', dashboard: '',
        portFree: true, expected: 'pass' },
      { name: 'model-free agent', agentAdapter: 'deterministic', dashboard: 'dashboard-id',
        portFree: false, expected: undefined },
    ] as const) {
      const selected = request(root, ['--agent-adapter', scenario.agentAdapter,
        '--image', EXACT_IMAGE]);
      const report = runPreflight(selected, {
        run: (file, args) => {
          if (args[0] === 'info') return dockerInfo();
          if (args[0] === 'compose') return args.includes('dashboard') ? scenario.dashboard : '2.40.0';
          if (args[0] === 'ps') return '';
          if (args[0] === 'image') return args.includes('{{.Os}}/{{.Architecture}}')
            ? 'linux/amd64' : `${IMAGE_ID}\n`;
          if (file === 'curl') return '200';
          throw new Error(`unexpected docker command: ${args.join(' ')}`);
        },
        env: { STACK_BENCH_APPLIANCE: '1', STACK_BENCH_CONTROLLER_IMAGE: EXACT_IMAGE,
          STACK_BENCH_NPM_REGISTRY: 'http://127.0.0.1:4873', ANTHROPIC_API_KEY: '<test-present>' },
        home: root, now: Date.parse('2026-08-12T12:00:00.100Z'),
        statfs: () => ({ bavail: 20n, bsize: 1024n ** 3n }), pidsOnPort: () => [],
        probePort: port => ({ free: port === 7331 ? scenario.portFree : true }),
      });
      assert.equal(report.checks.find(check => check.id === 'admission.dashboard')?.status,
        scenario.expected, scenario.name);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a rotating interactive credential cannot satisfy preflight', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-credential-'));
  try {
    const credential = join(root, '.claude', '.credentials.json');
    mkdirSync(join(root, '.claude'));
    writeFileSync(credential, '{}');
    const selected = parsePreflightArgs(['node', 'preflight.js', '--backend', 'stub',
      '--track', 'loop', '--levels', '1', '--agent-adapter', 'claude-code',
      '--results-dir', root]);
    const run: DockerCommand = (_file, args) => {
      if (args[0] === 'info') return dockerInfo();
      if (args[0] === 'compose') return '2.40.0'; if (args[0] === 'ps') return '';
      if (args[0] === 'image') return args[3] === '{{.Os}}/{{.Architecture}}'
        ? 'linux/amd64' : `${IMAGE_ID}\n`;
      throw new Error(`unexpected docker command: ${args.join(' ')}`);
    };
    const report = runPreflight(selected, { run, now: Date.parse('2026-08-12T12:00:00.100Z'),
      env: {}, home: root, statfs: () => ({ bavail: 20n, bsize: 1024n ** 3n }),
      pidsOnPort: () => [], probePort: () => ({ free: true }) });
    assert.equal(report.ok, false);
    assert.equal(requiredCheck(report, 'agent.credentials').status, 'fail');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('long-lived subscription token is mounted read-only and checked without exposing its value', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-subscription-token-'));
  try {
    const tokenFile = join(root, 'claude-token');
    writeFileSync(tokenFile, 'must-not-appear-in-report\n');
    const selected = parsePreflightArgs(['node', 'preflight.js', '--backend', 'stub',
      '--track', 'loop', '--levels', '1', '--agent-adapter', 'claude-code',
      '--results-dir', root, '--smoke']);
    let dockerArgs: string[] | undefined;
    const run: DockerCommand = (_file, args) => {
      if (args[0] === 'info') return dockerInfo();
      if (args[0] === 'compose') return '2.40.0'; if (args[0] === 'ps') return '';
      if (args[0] === 'image') return args[3] === '{{.Os}}/{{.Architecture}}'
        ? 'linux/amd64' : `${IMAGE_ID}\n`;
      if (args[0] === 'run') {
        dockerArgs = args;
        const mount = requiredArgument(args, args.indexOf('-v') + 1).split(':/results')[0];
        assert.ok(mount);
        writeFileSync(join(mount, requiredArgument(args, args.length - 1)), 'container-write-ok');
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
    assert.equal(requiredCheck(report, 'agent.authentication').status, 'pass');
    assert.ok(dockerArgs);
    assert.match(requiredArgument(dockerArgs, dockerArgs.indexOf('--mount') + 1),
      /src=.*claude-token,dst=\/run\/secrets\/agent-credential,readonly$/);
    assert.deepEqual(JSON.parse(requiredArgument(dockerArgs, dockerArgs.length - 2)), {
      name: 'CLAUDE_CODE_OAUTH_TOKEN', file: '/run/secrets/agent-credential',
    });
    assert.doesNotMatch(JSON.stringify({ report, dockerArgs }), /must-not-appear-in-report/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('preflight fails before launch when a stack default skill document is absent', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-preflight-materials-'));
  try {
    for (const [guidance, missing, skills] of [
      ['prescribed', 'typescript-server', ['typescript-server', 'typescript-client']],
      ['neutral', 'typescript-client', ['typescript-server', 'typescript-client']],
    ] as const) {
      const selected = parsePreflightArgs(['node', 'preflight.js', '--backend', 'spacetime',
        '--track', 'ecommerce', '--levels', '1', '--agent-adapter', 'claude-code',
        '--guidance', guidance, '--results-dir', root]);
      const report = runPreflight(selected, {
        run: () => { throw new Error('Docker unavailable in this focused test'); },
        env: { ANTHROPIC_API_KEY: '<test-present>' }, home: root,
        exists: path => !String(path).includes(`${join('skills', missing)}`),
        statfs: () => ({ bavail: 20n, bsize: 1024n ** 3n }), pidsOnPort: () => [],
        probePort: () => ({ free: true }),
      });
      const materials = requiredCheck(report, 'materials.spacetime.skills');
      assert.equal(materials.status, 'fail');
      assert.deepEqual(stringArray(materials.evidence, 'skills'), skills);
      assert.equal(stringArray(materials.evidence, 'missing').length, 1);
    }
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
    assert.equal(requiredCheck(report, 'ambient.stack_bench_lease_token').status, 'fail');
    assert.equal(requiredCheck(report, 'docker.engine').status, 'fail');
    assert.equal(requiredCheck(report, 'port.stub.vite').status, 'fail');
    assert.doesNotMatch(JSON.stringify(report), /must-not-be-reported/);
    assert.equal(requiredCheck(report, 'outbound.container').status, 'warn');
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
    assert.equal(requiredCheck(accepted, 'ambient.stack_bench_supervisor_state').status, 'pass');

    writeFileSync(supervisorState, '{}');
    const stale = runPreflight({ ...request(root), supervisorState }, {
      ...dependencies, env: { STACK_BENCH_SUPERVISOR_STATE: supervisorState },
    });
    assert.equal(requiredCheck(stale, 'ambient.stack_bench_supervisor_state').status, 'fail');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('preflight argument parsing rejects ambiguous ranges and missing backends', () => {
  assert.throws(() => parsePreflightArgs(['node', 'preflight.js']), /--backend is required/);
  assert.throws(() => parsePreflightArgs(['node', 'preflight.js', '--backend', 'stub',
    '--levels', '3-1']), /--levels/);
  assert.throws(() => parsePreflightArgs(['node', 'preflight.js', '--backend', 'stub',
    '--mystery']), /Unknown option/);
  assert.equal(parsePreflightArgs(['node', 'preflight.js', '--backend', 'spacetime',
    '--guidance', 'neutral']).guidance, 'neutral');
  assert.throws(() => parsePreflightArgs(['node', 'preflight.js', '--backend', 'spacetime',
    '--guidance', 'unknown']), /--guidance/);
  assert.throws(() => parsePreflightArgs(['node', 'preflight.js', '--backend', 'spacetime',
    '--guidance', '']), /--guidance/);
  assert.throws(() => parsePreflightArgs(['node', 'preflight.js', '--backend', 'spacetime',
    '--parallelism', '']), /--parallelism/);
  assert.equal(parsePreflightArgs(['node', 'preflight.js', '--backend', 'postgres',
    '--track', 'ecommerce', '--levels', '1', '--recipe', 'ecommerce.sequential-l1'])
    .recipe, 'ecommerce.sequential-l1');
  assert.throws(() => parsePreflightArgs(['node', 'preflight.js', '--backend', 'postgres',
    '--track', 'ecommerce', '--levels', '1-2', '--recipe', 'ecommerce.sequential-l1']),
  /exactly one/);
});

test('appliance preflight defaults to its configured persistent result directory', () => {
  const resultsDir = resolve(tmpdir(), 'stack-bench-appliance-results');
  const parsed = parsePreflightArgs(['node', 'preflight.js', '--backend', 'postgres'], {
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
        : args[0] === 'compose' ? '2.40.0' : args[0] === 'ps' ? ''
          : args[3] === '{{.Os}}/{{.Architecture}}' ? 'linux/amd64' : IMAGE_ID,
      now: Date.parse('2026-08-12T12:00:00Z'), env: {}, home: root, pidsOnPort: () => [],
      probePort: () => ({ free: true }),
    });
    assert.equal(report.ok, false);
    assert.equal(requiredCheck(report, 'request.scope').status, 'fail');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('loopback port proof detects a listener without relying on process visibility', async t => {
  const { createServer } = await import('node:net');
  const server = createServer();
  await new Promise<void>((resolveListen, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolveListen);
  });
  t.after(() => server.close());
  const address = server.address();
  assert.ok(address && typeof address !== 'string');
  assert.deepEqual(probeLoopbackPort(address.port), { free: false });
});
