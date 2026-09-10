import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { MUTATION_GRADE_MAX_TIMEOUT_MS, mutationControlArgv, mutationControlTimeoutMs,
  mutationGradeTimeoutMs } from '../src/evidence/mutation-control.js';
import { loadTrack } from '../src/composition/tracks.js';
import { STACK_BENCH_ROOT, compiledEntrypoint } from '../src/package-root.js';
import { mutationClientCommand, mutationFailureMessage, mutationGradeArguments, restoreMutationSource,
  retainMutationGrade, resetMutationDatabase } from '../grader/mutation-test.js';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.js';
import { createBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';
import type { TextCommandExecutor } from '../src/runtime/command-executor.js';
import { parseGradeArgs } from '../grader/grade.js';
import { createArtifact } from '../src/evidence/artifacts.js';

test('mutation reset stops hosted apps before clearing data and fails closed', async t => {
  const root = mkdtempSync(join(tmpdir(), 'mutation-reset-order-'));
  const path = join(root, 'lease.json');
  const priorPath = process.env.STACK_BENCH_LEASE;
  const priorToken = process.env.STACK_BENCH_LEASE_TOKEN;
  t.after(() => {
    if (priorPath === undefined) delete process.env.STACK_BENCH_LEASE;
    else process.env.STACK_BENCH_LEASE = priorPath;
    if (priorToken === undefined) delete process.env.STACK_BENCH_LEASE_TOKEN;
    else process.env.STACK_BENCH_LEASE_TOKEN = priorToken;
    rmSync(root, { recursive: true, force: true });
  });
  for (const backend of ['postgres', 'mongodb'] as const) {
    const lease = createBackendLease({ runId: 'mutation-reset', backend,
      track: 'ecommerce', runIndex: 0, database: 'mutation_reset',
      container: { name: 'database', id: 'a'.repeat(64) } });
    lease.state = 'active';
    writeBackendLease(path, lease);
    process.env.STACK_BENCH_LEASE = path;
    process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
    const calls: string[] = [];
    let failure: string | null = null;
    const adapter = STACK_ADAPTER_REGISTRY.get(backend);
    const lifecycle = adapter.lifecycle as typeof adapter.lifecycle & {
      control: NonNullable<typeof adapter.lifecycle.control> };
    assert.equal(typeof lifecycle.control, 'function');
    t.mock.method(lifecycle, 'control', async ({ mode }: { mode: string }) => {
      calls.push(mode);
      if (failure === mode) throw new Error(`${mode} failed`);
    });
    t.mock.method(adapter.reset, 'run', () => {
      calls.push('reset');
      if (failure === 'reset') throw new Error('reset failed');
    });
    const args = { backend, app: root, track: 'ecommerce', reseedOnReset: true,
      restartSpec: { backend, app: root, port: 3000, probe: '/' } };
    await resetMutationDatabase(args, null);
    assert.deepEqual(calls.splice(0), ['stop', 'reset', 'start']);
    for (const phase of ['stop', 'reset']) {
      failure = phase;
      await assert.rejects(resetMutationDatabase(args, null), new RegExp(`${phase} failed`));
      assert.deepEqual(calls.splice(0), phase === 'stop' ? ['stop'] : ['stop', 'reset']);
    }
    await assert.rejects(resetMutationDatabase({ ...args, restartSpec: undefined }, null),
      /requires a lease-authenticated --restart-spec/);
    assert.deepEqual(calls, [], 'missing control must not clear the database');
  }
});

test('mutation grading retains each report, including errors, without reusing stale output', async t => {
  const root = mkdtempSync(join(tmpdir(), 'mutation-retained-grades-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const output = join(root, 'control.json');
  const receipts: Array<{ status: string; report: { path: string; sha256: string | null } }> = [];
  const record = (receipt: typeof receipts[number]) => receipts.push(structuredClone(receipt));
  const context = { scenario: 'scenario.json', mutationId: null };
  await retainMutationGrade(output, context, async path => {
    assert.equal(existsSync(path), false);
    writeFileSync(path, '{"baseline":true}');
  }, record);
  const baseline = receipts.at(-1)!;
  assert.equal(baseline.status, 'returned');
  assert.ok(baseline.report);
  const failure = new Error('deadline');
  await assert.rejects(retainMutationGrade(output, { ...context, mutationId: 'mutant' }, async path => {
    assert.equal(existsSync(path), false);
    writeFileSync(path, '{"partial":true}');
    throw failure;
  }, record), error => error === failure);
  const failed = receipts.at(-1)!;
  assert.equal(failed.status, 'threw');
  assert.ok(failed.report);
  assert.notEqual(failed.report.path, baseline.report.path);
  for (const receipt of [baseline, failed]) {
    const file = join(root, receipt.report!.path);
    assert.equal(createHash('sha256').update(readFileSync(file)).digest('hex'), receipt.report!.sha256);
  }
  await assert.rejects(retainMutationGrade(output, context, async path => {
    assert.equal(existsSync(path), false);
    throw failure;
  }, record));
  assert.equal(receipts.at(-1)!.report.sha256, null, 'a missing report must not reuse earlier grade bytes');
  assert.equal(existsSync(join(root, receipts.at(-1)!.report.path)), false);
  assert.ok(receipts[0]!.report.path, 'a running receipt identifies its report before the grader starts');
  assert.equal(readFileSync(join(root, baseline.report.path), 'utf8'), '{"baseline":true}');
  assert.equal(readFileSync(join(root, failed.report.path), 'utf8'), '{"partial":true}');
  assert.deepEqual(receipts.map(item => item.status),
    ['running', 'returned', 'running', 'threw', 'running', 'threw']);
});

test('mutation client builds and dist copies run only as the exact leased app owner', t => {
  const root = mkdtempSync(join(tmpdir(), 'mutation-client-owner-'));
  const path = join(root, 'lease.json');
  const priorPath = process.env.STACK_BENCH_LEASE;
  const priorToken = process.env.STACK_BENCH_LEASE_TOKEN;
  t.after(() => {
    if (priorPath === undefined) delete process.env.STACK_BENCH_LEASE;
    else process.env.STACK_BENCH_LEASE = priorPath;
    if (priorToken === undefined) delete process.env.STACK_BENCH_LEASE_TOKEN;
    else process.env.STACK_BENCH_LEASE_TOKEN = priorToken;
    rmSync(root, { recursive: true, force: true });
  });
  const lease = createBackendLease({ runId: 'mutation-client', backend: 'postgres',
    track: 'ecommerce', runIndex: 0, database: 'mutation_client',
    container: { name: 'database', id: 'a'.repeat(64) } });
  const id = 'b'.repeat(64);
  lease.state = 'active';
  lease.resources.buildContainer = { name: 'build', id, running: true, owned: true,
    image: `sha256:${'c'.repeat(64)}`,
    resourceLimits: { cpuCount: 2, memoryBytes: 1024, memorySwapBytes: 1024, pids: 32 } };
  writeBackendLease(path, lease);
  process.env.STACK_BENCH_LEASE = path;
  process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
  let observedId = id;
  const calls: string[][] = [];
  const exec: TextCommandExecutor = (command, args, options) => {
    assert.equal(command, 'docker');
    if (args[0] === 'inspect') return observedId;
    assert.equal(options.timeout, 3000);
    calls.push([...args]);
    return '';
  };
  for (const [command, ...args] of [
    ['npm', 'run', 'build'],
    ['cp', '-R', '--', '/app/client/dist', '/tmp/clean-client'],
    ['rm', '-rf', '--', '/app/client/dist'],
    ['cp', '-R', '--', '/tmp/clean-client', '/app/client/dist'],
  ]) {
    mutationClientCommand('postgres', command!, args, 3000, exec);
    assert.deepEqual(calls.at(-1), ['exec', '--user', '10001:10001',
      '-e', 'HOME=/home/developer', '-e', 'USER=developer', '-w', '/app/client', id,
      'sh', '-c', 'umask 022; exec "$@"', 'application-command', command, ...args]);
  }
  const before = calls.length;
  observedId = 'd'.repeat(64);
  assert.throws(() => mutationClientCommand('postgres', 'npm', ['run', 'build'], 3000, exec),
    /changed after lease creation/);
  process.env.STACK_BENCH_LEASE_TOKEN = 'wrong';
  assert.throws(() => mutationClientCommand('postgres', 'rm', ['-rf', '/app/client/dist'], 3000, exec),
    /token does not match/);
  assert.equal(calls.length, before);
});

test('mutation restore verifies original contents and keeps the backup if writing fails', () => {
  const directory = mkdtempSync(join(tmpdir(), 'mutation-restore-'));
  const target = join(directory, 'source.ts');
  const backup = `${target}.mutation-backup`;
  const original = 'const name = "café";\n';
  try {
    writeFileSync(target, 'mutant');
    writeFileSync(backup, original);
    restoreMutationSource({ target, backup, original });
    assert.equal(readFileSync(target, 'utf8'), original);
    assert.equal(existsSync(backup), false);
    rmSync(target);
    mkdirSync(target);
    writeFileSync(backup, original);
    assert.throws(() => restoreMutationSource({ target, backup, original }));
    assert.equal(readFileSync(backup, 'utf8'), original);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test('mutation cleanup diagnostics retain the original error and restore failure', () => {
  const message = mutationFailureMessage(new AggregateError([
    new Error('mutation grade failed'), new Error('cannot restore source: EPERM copyfile'),
  ], 'mutation cleanup failed; do not reuse this app source'));
  assert.match(message, /mutation grade failed/);
  assert.match(message, /EPERM copyfile/);
  assert.match(message, /do not reuse/);
});

test('mutation grader selection survives CLI parsing and strict artifact validation', () => {
  const selectedCheckKeys = ['ecommerce.spec.state-durability.staff-role-reload.621a',
    'ecommerce.spec.access-control.staff-role-boundary.621b'];
  const input = { app: '/app', url: 'http://app', level: '3', backend: 'postgres',
    track: 'ecommerce', spec: '/staff-roles.json', recipe: 'ecommerce.progression-catalog',
    expectedRecipeSha256: 'a'.repeat(64), selectedCheckKeys };
  const parse = (keys: string[]) => parseGradeArgs(['node',
    ...mutationGradeArguments({ ...input, selectedCheckKeys: keys }, '/grade.json')]);
  const parsed = parse(selectedCheckKeys);
  assert.deepEqual(parsed.selectedCheckKeys, selectedCheckKeys);
  assert.equal(parsed.expectedRecipeSha256, input.expectedRecipeSha256);
  assert.match(parsed.selectionSha256 ?? '', /^[a-f0-9]{64}$/);
  assert.equal(parsed.selectionSha256, parse([...selectedCheckKeys].reverse()).selectionSha256);
  assert.notEqual(parsed.selectionSha256, parse(selectedCheckKeys.slice(0, 1)).selectionSha256);
  const payload = { total: 0, max: 0, features: [], selection: {
    sha256: parsed.selectionSha256,
    checks: selectedCheckKeys.map(stableKey => ({ stableKey, points: 2 })),
  } };
  assert.doesNotThrow(() => createArtifact({ kind: 'grade', id: 'mutation-selection', payload }));
  assert.throws(() => createArtifact({ kind: 'grade', id: 'missing-selection-identity',
    payload: { ...payload, selection: { checks: payload.selection.checks } } }),
  /grade payload.selection is invalid/);
});

test('campaign-bound mutation grading forwards the run level and exact recipe', () => {
  const manifest = join(STACK_BENCH_ROOT, 'grader', 'mutations', 'mongodb-ecommerce.json');
  const recipeTask = { schemaVersion: 3,
    recipe: { id: 'ecommerce.sequential-l1' },
    selection: {}, task: {} };
  const args = { out: 'output', mutations: manifest, backend: 'mongodb',
    track: 'ecommerce', levelList: [1], runIndex: 0, parentAttemptId: 'campaign-attempt',
    recipe: null, recipeTasks: new Map([[1, { request: recipeTask }]]) };
  const argv = mutationControlArgv(args, 'app', 'http://localhost:5173',
    loadTrack('ecommerce'));
  assert.equal(argv[argv.indexOf('--recipe') + 1], 'ecommerce.sequential-l1');
  assert.equal(argv[argv.indexOf('--level') + 1], '1');
  assert.deepEqual(JSON.parse(argv[argv.indexOf('--restart-spec') + 1] ?? ''), {
    backend: 'mongodb', app: 'app', port: 6723, probe: '',
  });
  assert.throws(() => mutationControlArgv({ ...args,
    recipe: 'ecommerce.sequential-l2' }, 'app', 'http://localhost:5173',
    loadTrack('ecommerce')), /does not match bound task/);
});

test('mutation grading receives the exact scored checks selected for the run', () => {
  const manifest = join(STACK_BENCH_ROOT, 'grader', 'mutations', 'mongodb-ecommerce.json');
  const args = { out: 'output', mutations: manifest, backend: 'mongodb',
    track: 'ecommerce', levelList: [3], runIndex: 0, parentAttemptId: 'selected-attempt',
    recipe: null, recipeTasks: new Map([[3, {
      request: { schemaVersion: 3,
        recipe: { id: 'ecommerce.progression-catalog' },
        selection: {}, task: {} },
      selection: { scoredChecks: [
        { stableKey: 'ecommerce.inventory-operations.warehouse-transfer.2a' },
        { stableKey: 'ecommerce.inventory-operations.stock-conservation.202a' },
      ] },
    }]]) };
  const argv = mutationControlArgv(args, 'app', 'http://localhost:5173',
    loadTrack('ecommerce'));
  assert.deepEqual(argv.flatMap((value, index) => value === '--selected-check'
    ? [argv[index + 1]] : []), [
    'ecommerce.inventory-operations.warehouse-transfer.2a',
    'ecommerce.inventory-operations.stock-conservation.202a',
  ]);
});

test('mutation control timeout follows its explicit runtime budget', () => {
  assert.equal(mutationControlTimeoutMs(), 80 * 60_000);
  assert.equal(mutationControlTimeoutMs(15), 35 * 60_000);
  assert.throws(() => mutationControlTimeoutMs(0), /positive number/);
});

test('each mutation grade uses only the remaining batch time', () => {
  const now = 1_000_000;
  assert.equal(mutationGradeTimeoutMs(now + 30_000, now), 30_000);
  assert.equal(mutationGradeTimeoutMs(now + MUTATION_GRADE_MAX_TIMEOUT_MS + 1, now),
    MUTATION_GRADE_MAX_TIMEOUT_MS);
  assert.equal(mutationGradeTimeoutMs(now, now), 0);
  assert.equal(mutationGradeTimeoutMs(now - 1, now), 0);
  assert.throws(() => mutationGradeTimeoutMs(Number.NaN, now), /must be finite/);
});

test('mutation shard coordinates reach the mutation runner together', () => {
  const manifest = join(STACK_BENCH_ROOT, 'grader', 'mutations', 'mongodb-ecommerce.json');
  const args = { out: 'output', mutations: manifest, backend: 'mongodb',
    track: 'ecommerce', levelList: [1], runIndex: 4, parentAttemptId: 'parallel-attempt',
    recipe: null, recipeTasks: new Map(), mutationShardIndex: 2, mutationShardCount: 4 };
  const argv = mutationControlArgv(args, 'app', 'http://localhost:5173',
    loadTrack('ecommerce'));
  const index = argv.indexOf('--mutation-shard-index');
  assert.deepEqual(argv.slice(index, index + 4),
    ['--mutation-shard-index', '2', '--mutation-shard-count', '4']);
});

test('mutation checkpoint controls reach the mutation runner', () => {
  const manifest = join(STACK_BENCH_ROOT, 'grader', 'mutations', 'mongodb-ecommerce.json');
  const args = { out: 'output', mutations: manifest, backend: 'mongodb',
    track: 'ecommerce', levelList: [1], runIndex: 4, parentAttemptId: 'resume-attempt',
    recipe: null, recipeTasks: new Map(), mutationResumeFrom: 'prior.json',
    mutationCheckpointOut: 'next.json', mutationMaxRuntimeMinutes: 30,
    mutationImageId: 'sha256:image', mutationBaselineBundle: 'baseline.json',
    expectedMutationCalibration: { id: 'calibration', sha256: 'calibration-sha' } };
  const argv = mutationControlArgv(args, 'app', 'http://localhost:5173',
    loadTrack('ecommerce'));
  const after = (flag: string): string => {
    const value = argv[argv.indexOf(flag) + 1];
    if (value === undefined) throw new Error(`missing value for ${flag}`);
    return value;
  };
  assert.equal(after('--resume-from'), 'prior.json');
  assert.equal(after('--checkpoint-out'), 'next.json');
  assert.equal(after('--max-runtime-minutes'), '30');
  assert.equal(after('--image-id'), 'sha256:image');
  assert.equal(after('--baseline-bundle'), 'baseline.json');
  assert.deepEqual(JSON.parse(after('--expected-calibration-json')),
    args.expectedMutationCalibration);
});

test('a mismatched mutation fixture fails before acquiring any backend resource', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-mutation-preflight-'));
  const output = join(root, 'output');
  const manifest = join(root, 'mutations.json');
  const locks = join(tmpdir(), 'stack-bench-resource-locks');
  const lock = join(locks, `${createHash('sha256').update('slot:ecommerce:mongodb:run19').digest('hex')}.lock.json`);
  try {
    assert.equal(existsSync(lock), false, 'test slot is already leased');
    writeFileSync(join(root, 'source.txt'), 'fixture\n');
    // Use root as the explicit app; the manifest intentionally targets other
    // bytes. No Docker or database lookup should happen before this rejection.
    writeFileSync(manifest, JSON.stringify({ schemaVersion: 3,
      fixtureSha256: '0'.repeat(64), backend: 'mongodb', track: 'ecommerce',
      scenario: 'tracks/ecommerce/scenarios/01-contention.json', mutations: [] }));
    assert.throws(() => execFileSync(process.execPath, [compiledEntrypoint('commands', 'bench.js'),
      '--backend', 'mongodb', '--track', 'ecommerce', '--levels', '1',
      '--run-index', '19', '--app', root, '--out', output,
      '--agent-adapter', 'deterministic', '--mutations', manifest, '--no-media'],
    { encoding: 'utf8', stdio: 'pipe', timeout: 30_000 }), /targets fixture/);
    assert.equal(existsSync(lock), false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
