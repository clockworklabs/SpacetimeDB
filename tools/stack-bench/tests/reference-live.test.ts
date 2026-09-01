import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { parseReferenceQualificationArgs,
  mutationWorkerRequiresSiblingAbort, referenceQualificationContext,
  parallelMutationChildArgv, parallelMutationResourceLockKeys, preflightParallelMutationResources,
  parallelMutationResults, readParallelMutationWorker, referenceQualificationPaths,
  qualificationMutationManifest,
  companionReferenceArtifactPath,
  assertReleaseCandidateRepetitions,
  qualificationArtifactsOk,
  referenceQualificationRelease,
  referenceQualificationRunner,
  referenceQualificationSelectionArgs,
  referenceQualificationWorkRoot, referenceRunFromMutationBaseline,
  targetedMutationCheckKeys } from '../src/references/reference-live.js';
import { auditMutationWorkerRun, auditReferenceRun }
  from '../src/references/reference-qualification-audit.js';
import { runBounded } from '../src/runtime/bounded-process.js';
import { rescueSupervisedLease } from '../src/runtime/recovery.js';
import { emptyArtifactIdentities, readArtifact, writeArtifact, writeRunJson }
  from '../src/evidence/artifacts.js';
import { createCheckEvidence } from '../src/evidence/check-evidence.js';
import { readMutationManifest } from '../src/evidence/mutation-analysis.js';
import { createBoundRecipeTaskRequest } from '../src/composition/recipe-selection.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { loadTrack } from '../src/composition/tracks.js';
import { resolveFeatureCatalog } from '../src/progression/feature-catalog-selection.js';
import { resolveProgressionRecipeLevelSelection }
  from '../src/progression/progression-recipe-selection.js';
import type { ReferenceFixture } from '../src/references/reference-fixtures.js';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
const fixture: ReferenceFixture & { imported: { sourceSha256: string } } = {
  id: 'reference-live-test', backend: 'mongodb', track: 'ecommerce', level: 1,
  status: 'qualified',
  imported: { sourceSha256: 'a'.repeat(64) } };

function required<T>(value: T | null | undefined, description: string): T {
  if (value === null || value === undefined) throw new Error(`${description} is required`);
  return value;
}

function valuesAfter(argv: readonly string[], flag: string): string[] {
  return required(argv[argv.indexOf(flag) + 1], `${flag} value`).split(',');
}

function record(value: unknown, description: string): Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${description} must be an object`);
  }
  return Object.fromEntries(Object.entries(value));
}

test('reference qualification runs only the mutations selected by its check scope', () => {
  const path = 'grader/mutations/mongodb-ecommerce-2.0.1.json';
  const source = readMutationManifest(join(STACK_BENCH_ROOT, path));
  const ids = source.mutations.slice(0, 2).map(mutation => mutation.id);
  const context = { calibration: { mutations: [{ backend: 'mongodb', path,
    targets: ids.map(id => ({ id, stableKeys: [] })) }] } };
  const selected = qualificationMutationManifest({ ...fixture, id: 'selected-mutations',
    mutationManifests: ['grader/mutations/unused.json', path] }, context);

  assert.deepEqual(selected.mutations.map(mutation => mutation.id), ids);
  assert.deepEqual(qualificationMutationManifest({ ...fixture, id: 'targeted-mutation',
    mutationManifests: [path] }, context, [required(ids[1], 'second mutation id')])
    .mutations.map(mutation => mutation.id), [ids[1]]);
  assert.throws(() => qualificationMutationManifest({ ...fixture, id: 'missing-target',
    mutationManifests: [path] }, context, ['not-selected']), /targeted mutation selection is missing/);
  assert.throws(() => qualificationMutationManifest({ ...fixture, id: 'missing-mutation',
    mutationManifests: [path] }, { calibration: { mutations: [{ backend: 'mongodb', path,
      targets: [{ id: 'not-present' }] }] } }), /mutation selection is missing/);
  assert.throws(() => qualificationMutationManifest({ ...fixture, id: 'wrong-owner',
    mutationManifests: [] }, context), /does not own its calibrated mutation manifest/);
});

test('targeted mutation diagnostics grade only their scored target checks', () => {
  const context = { binding: { release: { checkCatalog: [
    { stableKey: 'check.a', points: 1 },
    { stableKey: 'check.b', points: 2 },
    { stableKey: 'check.control', points: 0 },
    { stableKey: 'check.outside', points: 1 },
  ] } }, selectedCheckKeys: ['check.a', 'check.b', 'check.control'] };
  const manifest = { mutations: [
    { id: 'break-b', targets: ['check.b', 'check.control'] },
  ] };
  assert.deepEqual(targetedMutationCheckKeys(context, manifest), ['check.b']);
  assert.throws(() => targetedMutationCheckKeys(context, { mutations: [
    { id: 'outside', targets: ['check.outside'] },
  ] }), /outside the run scope/);
  assert.throws(() => targetedMutationCheckKeys(context, { mutations: [
    { id: 'missing', targets: ['check.missing'] },
  ] }), /unknown checks/);
});

test('reference qualification requires an explicit valid stack scope', () => {
  const args = parseReferenceQualificationArgs(['node', 'reference-live.js', '--backend', 'postgres',
    '--track', 'ecommerce', '--level', '2']);
  assert.equal(args.track, 'ecommerce');
  assert.equal(args.level, 2);
  assert.equal(args.mutations, false);
  assert.equal(args.timeoutMinutes, 60);
  const mutationArgs = parseReferenceQualificationArgs(['node', 'reference-live.js', '--backend', 'postgres',
    '--mutations', '--release-candidate']);
  assert.equal(mutationArgs.mutations, true);
  assert.equal(mutationArgs.timeoutMinutes, 120);
  assert.equal(mutationArgs.mutationMaxRuntimeMinutes, 60);
  assert.throws(() => parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'postgres', '--mutations']), /requires --release-candidate/);
  assert.throws(() => parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'postgres', '--release-candidate']), /requires --mutations/);
  const targeted = parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'postgres', '--mutations', '--mutation-id', 'one-defect']);
  assert.deepEqual(targeted.mutationIds, ['one-defect']);
  assert.equal(targeted.releaseCandidate, undefined);
  assert.throws(() => parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'postgres', '--mutation-id', 'one-defect']), /requires --mutations/);
  assert.throws(() => parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'postgres', '--mutations', '--release-candidate', '--mutation-id', 'one-defect']),
  /cannot select individual mutations/);
  assert.equal(parseReferenceQualificationArgs(['node', 'reference-live.js', '--backend', 'postgres',
    '--mutations', '--release-candidate', '--timeout-minutes', '120']).timeoutMinutes, 120);
  assert.equal(parseReferenceQualificationArgs(['node', 'reference-live.js', '--backend', 'postgres',
    '--mutations', '--release-candidate', '--timeout-minutes', '60', '--mutation-max-runtime-minutes', '30'])
    .mutationMaxRuntimeMinutes, 30);
  assert.throws(() => parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'postgres', '--mutations', '--release-candidate', '--timeout-minutes', '60']), /plus 20 minutes/);
  assert.throws(() => parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'postgres', '--mutations', '--release-candidate', '--timeout-minutes', '181']), /through 180/);
  assert.equal(parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'postgres', '--track', 'ecommerce', '--level', '3']).level, 3);
  assert.equal(parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'postgres', '--feature-catalog', 'ecommerce.questlines@2.0.1'])
    .featureCatalog, 'ecommerce.questlines@2.0.1');
  assert.throws(() => parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'postgres', '--track', 'ecommerce', '--level', '4']), /declared/);
  assert.equal(parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'postgres', '--track', 'ecommerce', '--level', '1',
    '--recipe', 'ecommerce.sequential-l1@2.5.0']).recipe, 'ecommerce.sequential-l1@2.5.0');
  assert.equal(parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'postgres', '--repetitions', '1']).repetitions, 1);
  assert.throws(() => parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'postgres', '--repetitions', '0']), /positive integer/);
});

test('parallel Spacetime qualification derives an isolated listener port from the run index', () => {
  const first = parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'spacetime', '--run-index', '0']);
  const parallel = parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'spacetime', '--run-index', '14']);
  const explicit = parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'spacetime', '--run-index', '14', '--spacetime-port', '4411']);

  assert.equal(first.spacetimePort, 3310);
  assert.equal(parallel.spacetimePort, 3324);
  assert.notEqual(first.spacetimePort, parallel.spacetimePort);
  assert.equal(explicit.spacetimePort, 4411);
  assert.throws(() => parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'spacetime', '--mutations', '--release-candidate', '--mutation-workers', '2',
    '--spacetime-port', '65535']), /worker offsets/);
});

test('parallel mutation qualification reserves bounded slots and exact child shards', () => {
  const args = parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'mongodb', '--mutations', '--release-candidate', '--mutation-workers', '4', '--run-index', '8']);
  assert.equal(args.mutationWorkers, 4);
  assert.throws(() => parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'mongodb', '--mutation-workers', '2']), /requires --mutations/);
  assert.throws(() => parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'mongodb', '--mutations', '--release-candidate', '--mutation-workers', '2', '--run-index', '20']),
  /run-index cap/);

  const argv = parallelMutationChildArgv(args,
    { binding: { release: { id: 'ecommerce.sequential-l2', version: '1.5.0' } } },
    { artifactPath: '/results/w3.json', baselineBundle: '/results/clean/bundle.json',
      workerIndex: 2, workerCount: 4 });
  const after = (flag: string) => required(argv[argv.indexOf(flag) + 1], `${flag} value`);
  assert.equal(after('--run-index'), '10');
  assert.equal(after('--mutation-shard-index'), '2');
  assert.equal(after('--mutation-shard-count'), '4');
  assert.equal(after('--repetitions'), '1');
  assert.equal(after('--out'), '/results/w3.json');
  assert.equal(argv.includes('--reference-mutation-only'), true);
  assert.equal(after('--mutation-baseline-bundle').replaceAll('\\', '/'),
    '/results/clean/bundle.json');

  args.mutationIds = ['one-defect'];
  const targetedArgv = parallelMutationChildArgv(args,
    { binding: { release: { id: 'ecommerce.sequential-l2', version: '1.5.0' } } },
    { artifactPath: '/results/w3.json', baselineBundle: '/results/clean/bundle.json',
      workerIndex: 2, workerCount: 4 });
  assert.equal(targetedArgv[targetedArgv.indexOf('--mutation-id') + 1], 'one-defect');

  args.mutationCheckpointDir = '/results/checkpoints';
  const resumable = parallelMutationChildArgv(args,
    { binding: { release: { id: 'ecommerce.sequential-l2', version: '1.5.0' } } },
    { artifactPath: '/results/w3.json', baselineBundle: '/results/clean/bundle.json',
      workerIndex: 2, workerCount: 4 });
  assert.equal(required(resumable[resumable.indexOf('--mutation-checkpoint') + 1], 'mutation checkpoint')
    .replaceAll('\\', '/'), '/results/checkpoints/mongodb-worker-3.json');
  assert.equal(resumable[resumable.indexOf('--mutation-max-runtime-minutes') + 1], '60');
});

test('parallel mutation workers stop siblings only for unusable failures', () => {
  const complete = { assigned: ['one'], control: {
    checkpoint: { status: 'complete' }, results: [{ id: 'one', status: 'SURVIVED' }],
  } };
  assert.equal(mutationWorkerRequiresSiblingAbort({ ok: false }, complete), false,
    'a conclusive survivor is usable mutation evidence');
  assert.equal(mutationWorkerRequiresSiblingAbort({ ok: false }, {
    assigned: ['one'], control: { outcome: { kind: 'harness_failure' } },
  }), true);
  assert.equal(mutationWorkerRequiresSiblingAbort({ ok: false }, {
    assigned: ['one'], control: null,
  }), true);
});

test('mutation-only worker audit requires Docker, caught defects, and released resources', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-mutation-worker-audit-'));
  try {
    const identities = emptyArtifactIdentities({ stackAdapter: { id: 'mongodb' } });
    writeArtifact(join(root, 'mutation-control.json'), { kind: 'mutation_control', id: 'control',
      identities, payload: { fixtureSha256: fixture.imported.sourceSha256, ok: true,
        baseline: { total: 2, max: 2 }, summary: { caught: 1, completed: 1, total: 1,
          remaining: 0 }, results: [{ id: 'mutation', status: 'CAUGHT' }] } });
    writeArtifact(join(root, 'run.json'), { kind: 'benchmark_run', id: 'run', identities,
      payload: { backend: 'mongodb', track: 'ecommerce', setup: { isolation: {
        mode: 'container', imageId: 'sha256:image' } }, outcome: { kind: 'passed' },
        mutationControl: { ok: true }, backendLease: { state: 'released', resources: {
          buildContainer: { running: false }, locks: [{ key: 'slot', releasedAt: new Date().toISOString() }],
        } } } });
    const audited = auditMutationWorkerRun(root, fixture);
    assert.equal(audited.ok, true);
    assert.equal(audited.imageId, 'sha256:image');

    const failed = readArtifact<{ results: Array<{ status: string }> }>(join(root, 'mutation-control.json'));
    required(failed.payload.results[0], 'first mutation result').status = 'SURVIVED';
    writeFileSync(join(root, 'mutation-control.json'), `${JSON.stringify(failed)}\n`);
    assert.equal(auditMutationWorkerRun(root, fixture).ok, false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('parallel mutation preflight covers every worker slot and Spacetime listener', () => {
  const args = parseReferenceQualificationArgs(['node', 'reference-live.js',
    '--backend', 'spacetime', '--track', 'ecommerce', '--mutations', '--release-candidate',
    '--mutation-workers', '3', '--run-index', '8']);
  const keys = parallelMutationResourceLockKeys(args);
  assert.deepEqual(keys, [
    'listener:http://127.0.0.1:3318',
    'listener:http://127.0.0.1:3319',
    'listener:http://127.0.0.1:3320',
    'slot:ecommerce:spacetime:run10',
    'slot:ecommerce:spacetime:run8',
    'slot:ecommerce:spacetime:run9',
  ]);
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-parallel-preflight-'));
  try {
    for (const key of keys) {
      const digest = createHash('sha256').update(key).digest('hex');
      const path = join(root, `${digest}.lock.json`);
      writeFileSync(path, '{}\n');
      assert.throws(() => preflightParallelMutationResources(args,
        { STACK_BENCH_RESOURCE_LOCK_DIR: root }), new RegExp(key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
      rmSync(path);
    }
    assert.doesNotThrow(() => preflightParallelMutationResources(args,
      { STACK_BENCH_RESOURCE_LOCK_DIR: root }));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('parallel worker evidence is read only from its exact contained output', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-parallel-worker-'));
  try {
    const artifactPath = join(root, 'w1.json');
    const output = join(root, 'w1.runs', 'r1');
    mkdirSync(output, { recursive: true });
    const identities = emptyArtifactIdentities({
      fixture: { id: 'fixture', sha256: 'a'.repeat(64), state: 'candidate' },
      recipe: { id: 'recipe', version: '1.0.0', sha256: 'b'.repeat(64), state: 'candidate' },
      calibration: { id: 'calibration', version: '1.0.0', sha256: 'c'.repeat(64), state: 'draft' },
      stackAdapter: { id: 'mongodb' },
    });
    const mutationIds = ['first', 'second'];
    writeArtifact(join(output, 'mutation-control.json'), { kind: 'mutation_control', id: 'control',
      payload: { ok: true, shard: { index: 0, count: 1, mutationIds },
        results: mutationIds.toReversed().map(id => ({ id, status: 'CAUGHT' })) } });
    writeArtifact(artifactPath, { kind: 'reference_qualification', id: 'worker', identities,
      payload: { fixture: 'fixture', mutationControl: true, requiredRepetitions: 1, ok: true,
        runs: [{ ok: true, output: 'w1.runs/r1' }] } });
    const inspected = readParallelMutationWorker(artifactPath, { ok: true },
      { ...identities, workerIndex: 0, workerCount: 1 },
      { scenario: 'shared', mutations: mutationIds.map(id => ({ id })) });
    assert.deepEqual(inspected.failures, []);
    assert.deepEqual(required(inspected.control, 'worker mutation control').results
      ?.map(result => result.id), ['second', 'first']);

    writeArtifact(join(output, 'mutation-control.json'), { kind: 'mutation_control', id: 'control',
      payload: { ok: true } });
    const empty = readParallelMutationWorker(artifactPath, { ok: true },
      { ...identities, workerIndex: 0, workerCount: 1 },
      { scenario: 'shared', mutations: mutationIds.map(id => ({ id })) });
    assert.equal(empty.shardVerified, false);
    assert(empty.failures.includes('worker mutation shard is missing'));
    assert(empty.failures.includes('worker mutation results are missing'));

    writeArtifact(join(output, 'mutation-control.json'), { kind: 'mutation_control', id: 'control',
      payload: { ok: true, shard: { index: 0, count: 1, mutationIds },
        results: mutationIds.map(id => ({ id, status: 'CAUGHT' })) } });

    const escaped = readArtifact<{ runs: Array<{ output: string }> }>(artifactPath);
    required(escaped.payload.runs[0], 'worker run').output = '../outside';
    writeFileSync(artifactPath, JSON.stringify(escaped));
    const rejected = readParallelMutationWorker(artifactPath, { ok: true },
      { ...identities, workerIndex: 0, workerCount: 1 },
      { scenario: 'shared', mutations: mutationIds.map(id => ({ id })) });
    assert(rejected.failures.includes('worker run output escapes its artifact directory'));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('parallel mutation accounting keeps the expected assignment when a worker stops early', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-parallel-worker-failure-'));
  try {
    const artifactPath = join(root, 'w2.json');
    const output = join(root, 'w2.runs', 'r1');
    mkdirSync(output, { recursive: true });
    const identities = emptyArtifactIdentities({
      fixture: { id: 'fixture', sha256: 'a'.repeat(64), state: 'candidate' },
      recipe: { id: 'recipe', version: '1.0.0', sha256: 'b'.repeat(64), state: 'candidate' },
      calibration: { id: 'calibration', version: '1.0.0', sha256: 'c'.repeat(64), state: 'draft' },
      stackAdapter: { id: 'mongodb' },
    });
    const manifest = { mutations: [
      { id: 'first', scenario: 'a' }, { id: 'second', scenario: 'b' },
      { id: 'third', scenario: 'c' }, { id: 'fourth', scenario: 'd' },
    ] };
    writeArtifact(join(output, 'mutation-control.json'), { kind: 'mutation_control', id: 'control',
      payload: { ok: false, outcome: { kind: 'harness_failure', phase: 'mutation-control',
        reason: 'baseline deadline exceeded' } } });
    writeArtifact(artifactPath, { kind: 'reference_qualification', id: 'worker', identities,
      payload: { fixture: 'fixture', mutationControl: true, requiredRepetitions: 1, ok: false,
        runs: [{ ok: false, output: 'w2.runs/r1' }] } });

    const inspected = readParallelMutationWorker(artifactPath, { ok: false, code: 2 },
      { ...identities, workerIndex: 1, workerCount: 2 }, manifest);
    assert.deepEqual(inspected.assigned, ['second', 'fourth']);
    assert.equal(inspected.shardVerified, false);
    assert.equal(inspected.failures.includes('worker mutation shard does not match its assignment'), false);

    const missing = readParallelMutationWorker(join(root, 'missing.json'), { ok: false, code: 2 },
      { ...identities, workerIndex: 1, workerCount: 2 }, manifest);
    assert.deepEqual(missing.assigned, ['second', 'fourth']);
    assert(missing.failures.includes('worker artifact is missing'));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('partial parallel mutation accounting preserves completed shard results', () => {
  const manifest = { mutations: [
    { id: 'first', scenario: 'a' }, { id: 'second', scenario: 'b' },
    { id: 'third', scenario: 'c' }, { id: 'fourth', scenario: 'd' },
  ] };
  const completed = { shardVerified: true, control: { shard: {
    index: 0, count: 2, mutationIds: ['first', 'third'],
  }, results: [{ id: 'third', status: 'CAUGHT' }, { id: 'first', status: 'SURVIVED' }] } };
  const failed = { shardVerified: false, control: { ok: false } };

  assert.deepEqual(parallelMutationResults(manifest, [completed, failed])
    .map(result => result.id), ['first', 'third']);
  assert.deepEqual(parallelMutationResults(manifest, [completed, {
    shardVerified: true, control: { shard: {
      index: 1, count: 2, mutationIds: ['second', 'fourth'],
    }, results: [{ id: 'fourth', status: 'CAUGHT' }, { id: 'second', status: 'CAUGHT' }] },
  }]).map(result => result.id), ['first', 'second', 'third', 'fourth']);
});

test('reference qualification resolves the exact executable calibration identity', () => {
  const context = referenceQualificationContext({ ...fixture, id: 'ecommerce-reference-mongodb',
    imported: { sourceSha256: '24d445f18cdcb25b9ab06dd4f4582003b348edaf0be0b97c27ec9fbb06751b1e' } });
  assert.equal(record(context.identity, 'qualification identity').id, 'ecommerce.sequential-l1-calibration');
  assert.equal(record(context.identity, 'qualification identity').sha256, context.calibration.qualificationSha256);
});

test('reference qualification resolves the current calibration', () => {
  const context = referenceQualificationContext({ ...fixture, id: 'ecommerce-reference-mongodb',
    imported: { sourceSha256: '24d445f18cdcb25b9ab06dd4f4582003b348edaf0be0b97c27ec9fbb06751b1e' } },
  'ecommerce.sequential-l1@2.5.0');
  assert.equal(context.binding.release.version, '2.5.0');
  assert.equal(context.calibration.version, '2.5.0');
  assert.equal(context.calibration.state, 'draft');
});

test('modular reference qualification selects every exact check without prescribing specifications', () => {
  const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.sequential-l1@2.5.0');
  const argv = referenceQualificationSelectionArgs(binding);
  const featureIds = valuesAfter(argv, '--feature-module');
  const expectedSpecifications = valuesAfter(argv, '--expect-spec');
  const checkKeys = valuesAfter(argv, '--check');

  assert.equal(argv.includes('--request-spec'), false);
  assert.equal(checkKeys.length, 48);
  assert.equal(new Set(checkKeys).size, 48);
  const task = createBoundRecipeTaskRequest(binding,
    { featureIds, expectedSpecifications, checkKeys });
  assert.equal(task.selection.checks.length, 48);
  assert.equal(task.selection.scoredPoints, 58);
  assert.equal(task.selection.checks.filter(check => check.points === 0).length, 2);
  assert.equal(required(task.selection.specifications, 'task specifications').requested.length, 0);
  assert.equal(required(task.selection.specifications, 'task specifications').expected.length, expectedSpecifications.length);
});

test('progression reference qualification follows the catalog check selection', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 3, 'ecommerce.progression-depth3@2.0.1');
  const catalog = resolveFeatureCatalog('ecommerce.questlines@2.0.1', track);
  const selection = resolveProgressionRecipeLevelSelection(binding, catalog, 3,
    { cumulative: true });
  const argv = referenceQualificationSelectionArgs(binding, selection);
  assert.deepEqual(valuesAfter(argv, '--check').sort(), [...selection.grader.checkKeys].sort());
  assert.deepEqual(valuesAfter(argv, '--feature-module').sort(),
    [...selection.grader.selection.requested.features].sort());
  assert.deepEqual(valuesAfter(argv, '--expect-spec').sort(),
    [...selection.grader.selection.requested.specifications.expected].sort());
  assert.equal(required(valuesAfter(argv, '--task-mode')[0], 'task mode'), 'upgrade');
  assert.equal(selection.grader.checkKeys.length, 97);
  assert.equal(selection.grader.checkKeys.some(key => key.includes('automatic-reorder')), false);
  assert.deepEqual(referenceQualificationSelectionArgs(binding, selection,
    [required(selection.grader.checkKeys[0], 'first check key')]).filter((_value, index, argv) =>
    argv[index - 1] === '--check'), [required(selection.grader.checkKeys[0], 'first check key')]);
  const scoped = referenceQualificationRelease(binding.release, selection.grader.checkKeys);
  assert.equal(scoped.checkCatalog.length, 97);
  assert.throws(() => referenceQualificationRelease(binding.release,
    [...selection.grader.checkKeys, 'missing.check']), /unknown checks/);
});

test('depth-3 qualification compares the scoped graph identity', () => {
  const context = referenceQualificationContext({
    backend: 'mongodb', track: 'ecommerce', level: 6, id: 'ecommerce-reference-mongodb',
    status: 'qualified',
    imported: { sourceSha256: '24d445f18cdcb25b9ab06dd4f4582003b348edaf0be0b97c27ec9fbb06751b1e' },
  }, 'ecommerce.progression-depth3@2.0.1', {
    level: 3, featureCatalog: 'ecommerce.questlines@2.0.1',
  });
  assert.equal(record(context.featureCatalog, 'feature catalog').version, '2.0.1');
  assert.equal(required(context.selectedCheckKeys, 'selected check keys').length, 97);
  assert.equal(context.level, 3);
});

test('reference qualification keeps underlying runs beside the requested artifact', () => {
  const root = join(tmpdir(), 'stack-bench-reference-output-test');
  const paths = referenceQualificationPaths({ out: join(root, 'postgres-reference.json') }, 'ignored-id');
  assert.equal(paths.artifactPath, join(root, 'postgres-reference.json'));
  assert.equal(paths.artifactDirectory, root);
  assert.equal(paths.runsRoot, join(root, 'postgres-reference.runs'));
});

test('release mutation evidence gives its clean baseline a separate reference path', () => {
  const root = join(tmpdir(), 'stack-bench-companion-path-test');
  assert.equal(companionReferenceArtifactPath(join(root, 'ecommerce-l3-postgres-mutation.json')),
    join(root, 'ecommerce-l3-postgres-reference.json'));
  assert.equal(companionReferenceArtifactPath(join(root, 'custom.json')),
    join(root, 'custom-reference.json'));
});

test('release mutation evidence requires the calibrated repetition count', () => {
  const calibration = { qualification: { mutationRepetitions: 2 } };
  assert.doesNotThrow(() => assertReleaseCandidateRepetitions(
    { releaseCandidate: true, repetitions: 2 }, calibration));
  assert.throws(() => assertReleaseCandidateRepetitions(
    { releaseCandidate: true, repetitions: 1 }, calibration), /exactly 2/);
  assert.doesNotThrow(() => assertReleaseCandidateRepetitions(
    { releaseCandidate: false, repetitions: 1 }, calibration));
});

test('release qualification fails when either required artifact fails', () => {
  assert.equal(qualificationArtifactsOk({ ok: true }, { ok: true }), true);
  assert.equal(qualificationArtifactsOk({ ok: true }, { ok: false }), false);
  assert.equal(qualificationArtifactsOk({ ok: false }, { ok: true }), false);
  assert.equal(qualificationArtifactsOk({ ok: true }), true);
});

test('reference qualification uses the daemon-visible appliance work root', () => {
  assert.equal(referenceQualificationWorkRoot({ STACK_BENCH_WORK_DIR: '/var/lib/stack-bench/work' }),
    resolve('/var/lib/stack-bench/work'));
});

test('reference qualification records whether its controller is the supported Linux appliance', () => {
  assert.deepEqual(referenceQualificationRunner({ env: { STACK_BENCH_APPLIANCE: '1' },
    platform: 'linux', architecture: 'x64', dockerInfo: {
      ServerVersion: '29.1.2', OSType: 'linux', Architecture: 'x86_64',
      KernelVersion: '6.8.0-test', NCPU: 8, MemTotal: 16_000_000_000,
    } }), {
    schemaVersion: 1, mode: 'appliance', platform: 'linux', architecture: 'x64',
    dockerEngineVersion: '29.1.2', dockerOs: 'linux', dockerArchitecture: 'x86_64',
    kernelVersion: '6.8.0-test', cpuCount: 8, memoryBytes: 16_000_000_000,
  });
  assert.deepEqual(referenceQualificationRunner({ env: {}, platform: 'win32', architecture: 'x64' }), {
    schemaVersion: 1, mode: 'local-controller', platform: 'win32', architecture: 'x64',
  });
  assert.throws(() => referenceQualificationRunner({ env: { STACK_BENCH_APPLIANCE: '1' },
    platform: 'linux', architecture: 'x64', dockerInfo: {} }),
  /Docker daemon inspection did not return ServerVersion/);
});

function writeEvidence(root: string, { id, points, passed }: {
  id: string;
  points: number;
  passed: boolean;
}) {
  const stableKey = `test.reference.${id}`;
  const release = { id: 'test.reference', version: '1.0.0', contentSha256: 'b'.repeat(64),
    checkCatalog: [{ stableKey, points, source: 'scenarios/test.json', executionId: 'systems',
      featureId: 901, criterionId: id, packId: 'test.reference', checkGroupId: 'systems' }] };
  const identities = emptyArtifactIdentities({ recipe: { id: release.id, version: release.version,
    sha256: release.contentSha256, state: 'qualified' } });
  mkdirSync(join(root, 'grading'), { recursive: true });
  writeRunJson(join(root, 'run.json'), {
    id: 'reference-run', backend: 'mongodb', track: 'ecommerce',
    identities,
    setup: { isolation: { mode: 'container', imageId: 'sha256:test' } },
    outcome: { kind: 'passed' },
    levels: [{ level: 1, graded: true, contractPass: true, score: points, max: points }],
    backendLease: { state: 'released', resources: {
      buildContainer: { running: false }, locks: [{ key: 'slot:test', releasedAt: 'now' }],
    } },
  });
  const setupEvidence = createCheckEvidence({ status: 'passed', code: 'completed', phase: 'setup',
    startedAtMs: 1, completedAtMs: 2 });
  const evidence = createCheckEvidence({ status: passed ? 'passed' : 'failed',
    code: passed ? 'completed' : 'test_result', phase: 'assertion',
    startedAtMs: 1, completedAtMs: 2 });
  writeArtifact(join(root, 'grading', 'bundle.json'), { kind: 'grade_bundle', id: 'reference-bundle',
    identities,
    payload: { recipeRelease: release, selection: {
      recipe: { id: release.id, version: release.version, contentSha256: release.contentSha256 },
      checks: release.checkCatalog, reportedChecks: [stableKey], notRun: [],
    }, suites: {
      lint: { pass: true },
      systems: { features: [{ id: 901, setupEvidence,
        criteria: [{ id, stableKey, points, evidence }] }] },
    } } });
  return release;
}

test('reference qualification audits zero-point criteria and teardown evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-live-test-'));
  try {
    const release = writeEvidence(root, { id: '901a', points: 0, passed: true });
    const audit = auditReferenceRun(root, fixture, { release });
    assert.equal(audit.ok, true);
    assert.equal(audit.criteria, 1);
    assert.equal(audit.zeroPointCriteria, 1);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('reference qualification reports the exact level score', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-live-test-'));
  try {
    const release = writeEvidence(root, { id: '901a', points: 2, passed: true });
    const audit = auditReferenceRun(root, fixture, { release });
    assert.equal(audit.ok, true);
    assert.equal(audit.score, '2/2');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('reference qualification reports malformed evidence instead of throwing', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-live-test-'));
  try {
    mkdirSync(join(root, 'grading'), { recursive: true });
    writeFileSync(join(root, 'run.json'), '{');
    writeFileSync(join(root, 'grading', 'bundle.json'), '{}');
    const audit = auditReferenceRun(root, fixture);
    assert.equal(audit.ok, false);
    assert.match(audit.failures[0] ?? '', /qualification evidence is invalid/);
    writeFileSync(join(root, 'mutation-control.json'), '{}');
    const mutationAudit = auditMutationWorkerRun(root, fixture);
    assert.match(mutationAudit.failures[0] ?? '', /mutation evidence is invalid/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a mutation run reuses its stored clean baseline as reference evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-live-test-'));
  try {
    const baseline = join(root, 'clean');
    const release = writeEvidence(baseline, { id: '901a', points: 2, passed: true });
    const run = referenceRunFromMutationBaseline(root, {
      repetition: 1,
      output: 'workers',
      baselineOutput: 'clean',
      durationMs: 200,
      baselineDurationMs: 100,
      harnessSha256Before: 'b'.repeat(64),
      harnessSha256After: 'b'.repeat(64),
      baselineHarnessSha256Before: 'a'.repeat(64),
      baselineHarnessSha256After: 'a'.repeat(64),
    }, fixture, { release, level: 1,
      selectedCheckKeys: release.checkCatalog.map(check => check.stableKey) });
    assert.equal(run.ok, true);
    assert.equal(run.output, 'clean');
    assert.equal(run.durationMs, 100);
    assert.equal(run.score, '2/2');
    assert.equal(run.mutations, null);
    assert.equal(run.harnessSha256Before, 'a'.repeat(64));
    assert.equal(run.harnessSha256After, 'a'.repeat(64));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('reference qualification rejects a failed zero-point criterion', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-live-test-'));
  try {
    const release = writeEvidence(root, { id: '901a', points: 0, passed: false });
    const audit = auditReferenceRun(root, fixture, { release });
    assert.equal(audit.ok, false);
    assert.deepEqual(audit.failures, ['systems/901/901a did not pass']);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('mutation qualification requires a full baseline and every exact mutant caught', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-live-test-'));
  try {
    const release = writeEvidence(root, { id: '901a', points: 0, passed: true });
    const runPath = join(root, 'run.json');
    const run = JSON.parse(readFileSync(runPath, 'utf8'));
    run.payload.mutationControl = { ok: true };
    writeFileSync(runPath, JSON.stringify(run));
    writeArtifact(join(root, 'mutation-control.json'), { kind: 'mutation_control', id: 'mutation-run',
      payload: { ok: true, fixtureSha256: fixture.imported.sourceSha256,
        baseline: { total: 2, max: 2 }, summary: { caught: 1, total: 1 },
        results: [{ id: 'known-defect', status: 'CAUGHT' }] } });
    const passing = auditReferenceRun(root, fixture, { requireMutationControl: true, release });
    assert.equal(passing.ok, true);
    assert.deepEqual(passing.mutations, { caught: 1, total: 1 });

    const failed = JSON.parse(readFileSync(join(root, 'mutation-control.json'), 'utf8'));
    failed.payload.results[0].status = 'SURVIVED';
    writeFileSync(join(root, 'mutation-control.json'), JSON.stringify(failed));
    const audit = auditReferenceRun(root, fixture, { requireMutationControl: true, release });
    assert.equal(audit.ok, false);
    assert(audit.failures.includes('known-defect is SURVIVED'));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('reference qualification refuses identity-less or wrong same-sized check evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-live-test-'));
  try {
    const release = writeEvidence(root, { id: '901a', points: 1, passed: true });
    const bundlePath = join(root, 'grading', 'bundle.json');
    const bundle = JSON.parse(readFileSync(bundlePath, 'utf8'));
    bundle.payload.selection.checks[0].stableKey = 'wrong.same-sized.check';
    bundle.payload.selection.reportedChecks = ['wrong.same-sized.check'];
    bundle.payload.suites.systems.features[0].criteria[0].stableKey = 'wrong.same-sized.check';
    writeFileSync(bundlePath, JSON.stringify(bundle));
    const wrong = auditReferenceRun(root, fixture, { release });
    assert.equal(wrong.ok, false);
    assert(wrong.failures.includes('graded check catalog does not match the requested release'));

    const unidentified = auditReferenceRun(root, fixture);
    assert.equal(unidentified.ok, false);
    assert(unidentified.failures.includes('exact recipe release was not supplied to the qualification audit'));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('reference qualification terminates a child at its repetition deadline', async () => {
  const started = Date.now();
  const result = await runBounded(process.execPath,
    ['-e', 'setInterval(() => {}, 1000)'], {
      stdio: 'ignore', timeoutMs: 50,
      terminate: pid => process.kill(pid, 'SIGKILL'),
    });
  assert.equal(result.ok, false);
  assert.equal(result.timedOut, true);
  assert(Date.now() - started < 10_000, 'timed-out child was not terminated promptly');
});

test('bounded execution validates deadlines before opening logs', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-process-deadline-'));
  const stdout = join(root, 'stdout.log');
  const stderr = join(root, 'stderr.log');
  try {
    await assert.rejects(runBounded(process.execPath, [], {
      timeoutMs: 0, logs: { stdout, stderr },
    }), /timeoutMs must be a positive safe integer/);
    assert.equal(existsSync(stdout), false);
    assert.equal(existsSync(stderr), false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('bounded execution removes empty logs when process setup throws', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-process-setup-'));
  const stdout = join(root, 'stdout.log');
  const stderr = join(root, 'stderr.log');
  try {
    await assert.rejects(runBounded(null as unknown as string, [], {
      timeoutMs: 1_000, logs: { stdout, stderr },
    }));
    assert.equal(existsSync(stdout), false);
    assert.equal(existsSync(stderr), false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('parallel qualification cancellation terminates every bounded child', async () => {
  const cancellation = new AbortController();
  setTimeout(() => cancellation.abort(), 50);
  const result = await runBounded(process.execPath,
    ['-e', 'setInterval(() => {}, 1000)'], {
      stdio: 'ignore', timeoutMs: 10_000, signal: cancellation.signal,
      terminate: pid => process.kill(pid, 'SIGKILL'),
    });
  assert.equal(result.ok, false);
  assert.equal(result.cancelled, true);
  assert.equal(result.timedOut, false);
});

test('parallel qualification gives a worker time to release resources on cancellation',
  { skip: process.platform === 'win32' }, async () => {
    const root = mkdtempSync(join(tmpdir(), 'stack-bench-graceful-cancel-test-'));
    const marker = join(root, 'released');
    const cancellation = new AbortController();
    setTimeout(() => cancellation.abort(), 50);
    try {
      const result = await runBounded(process.execPath,
        ['-e', `process.on('SIGTERM',()=>{require('fs').writeFileSync(${JSON.stringify(marker)},'ok');process.exit(0)});setInterval(()=>{},1000)`],
        { cwd: root, env: process.env, stdio: 'ignore', timeoutMs: 10_000,
          signal: cancellation.signal, gracefulCancellationMs: 2_000 });
      assert.equal(result.cancelled, true);
      assert.equal(readFileSync(marker, 'utf8'), 'ok');
    } finally { rmSync(root, { recursive: true, force: true }); }
  });

test('bounded execution tees useful tails and caps durable process logs', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-process-log-'));
  try {
    const result = await runBounded(process.execPath,
      ['-e', 'process.stdout.write("abcdefgh"); process.stderr.write("actual failure\\n")'], {
        stdio: 'inherit', timeoutMs: 5_000,
        logs: { stdout: join(root, 'stdout.log'), stderr: join(root, 'stderr.log'), maxBytes: 5 },
      });
    assert.equal(result.ok, true);
    assert.equal(readFileSync(join(root, 'stdout.log'), 'utf8'), 'abcde');
    assert.equal(readFileSync(join(root, 'stderr.log'), 'utf8'), 'actua');
    const stdoutLog = required(required(result.logs, 'process logs').stdout, 'stdout process log');
    assert.deepEqual({ bytes: stdoutLog.bytes, retainedBytes: stdoutLog.retainedBytes,
      truncated: stdoutLog.truncated }, { bytes: 8, retainedBytes: 5, truncated: true });
    assert.match(result.stderrTail, /actual failure/);
    assert.equal(stdoutLog.sha256, createHash('sha256').update('abcde').digest('hex'));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('bounded execution refuses to overwrite an existing process log', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-exclusive-log-'));
  const stdout = join(root, 'stdout.log');
  writeFileSync(stdout, 'preserve');
  try {
    await assert.rejects(runBounded(process.execPath, ['-e', 'process.stdout.write("new")'], {
      stdio: 'ignore', timeoutMs: 5_000,
      logs: { stdout, stderr: join(root, 'stderr.log') },
    }), /EEXIST/);
    assert.equal(readFileSync(stdout, 'utf8'), 'preserve');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('supervisor accepts a deleted private lease only with matching released evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-supervisor-evidence-'));
  try {
    const state = join(root, 'supervisor.json');
    const output = join(root, 'output');
    mkdirSync(output);
    const runtimeDir = join(root, 'runtime');
    writeFileSync(state, JSON.stringify({ version: 2, runId: 'released-run', backend: 'mongodb',
      runtimeDir, leasePath: join(runtimeDir, 'backend-lease.json'),
      ownershipToken: 'private-token', output }));
    writeRunJson(join(output, 'run.json'), { id: 'released-run', backendLease: {
      runId: 'released-run', state: 'released', resources: {
        buildContainer: { running: false }, locks: [{ releasedAt: 'now' }],
      },
    } });
    assert.doesNotThrow(() => rescueSupervisedLease(state, output));
    const run = JSON.parse(readFileSync(join(output, 'run.json'), 'utf8'));
    run.payload.backendLease.state = 'active';
    writeFileSync(join(output, 'run.json'), JSON.stringify(run));
    assert.throws(() => rescueSupervisedLease(state, output), /without released run evidence/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
