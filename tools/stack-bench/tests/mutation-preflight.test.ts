import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { MUTATION_GRADE_MAX_TIMEOUT_MS, mutationControlArgv, mutationControlTimeoutMs,
  mutationGradeTimeoutMs } from '../src/evidence/mutation-control.js';
import { loadTrack } from '../src/composition/tracks.js';
import { STACK_BENCH_ROOT, compiledEntrypoint } from '../src/package-root.js';

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
    backend: 'mongodb', app: 'app', port: 6673, probe: '',
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
