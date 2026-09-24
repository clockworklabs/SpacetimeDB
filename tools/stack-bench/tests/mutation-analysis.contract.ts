import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';
import { classifyMutationResult, groupMutationsByScenario, mutationScenario,
  indexMutationReport, mutationFileEdits, validateMutationBaseline, isRetryableMutationBaseline,
  isRetryableMutationResult,
  releaseScenarioCheckKeys, resolveMutationFile, resolveMutationScenarioPath,
  reusableMutationBaseline,
  validateMutationDefinitions } from '../src/evidence/mutation-analysis.js';
import { createCheckEvidence } from '../src/evidence/check-evidence.js';
import { buildRecipeRelease, requireRecipeRelease as resolveRecipeRelease }
  from '../src/composition/recipe-release.js';
import { loadTrack } from '../src/composition/tracks.js';

type CriterionValue = boolean | 'inconclusive';

// A failed action record, as the grader stores the step a criterion stopped at.
const failedStep = (id: string) => {
  const { actor: _actor, actions: _actions, ...base } = createCheckEvidence({ status: 'failed',
    code: 'test_result', phase: 'assertion', startedAtMs: 1, completedAtMs: 2 });
  return { actor: null, evidence: { ...base, action: { id, version: '1' }, type: 'browser-observation-evidence',
    phase: 'execute', timing: { startedAtMs: 1, completedAtMs: 2, durationMs: 1, deadlineMs: 100 } } };
};

const report = (criteria: Record<string, CriterionValue>, setupError: string | null = null,
  failedAction = 'expect') => ({
  total: Object.values(criteria).filter(value => value === true).length,
  max: Object.keys(criteria).length,
  features: [{
    id: 7,
    criteria: Object.entries(criteria).map(([id, value]) => ({ id, stableKey: `check.${id}`,
      evidence: createCheckEvidence({
        status: value === true ? 'passed' : value === 'inconclusive' ? 'inconclusive' : 'failed',
        code: value === true ? 'completed' : 'test_result',
        phase: setupError ? 'setup' : 'assertion', summary: setupError,
        startedAtMs: 1, completedAtMs: 2,
        actions: value === false ? [failedStep(failedAction)] : [],
      }) })),
  }],
});
const mutation = { id: 'break-b', targets: ['check.b'] };

test('a clean scenario report is reused only for the exact run identity and check set', () => {
  const scenario = { total: 2, max: 2, selection: { checks: [
    { stableKey: 'check.a' }, { stableKey: 'check.b' },
  ] }, features: [] };
  const bundle = {
    backend: 'postgres', track: 'ecommerce', level: 3,
    source: { sha256: 'fixture' },
    recipeRelease: { id: 'recipe', contentSha256: 'recipe-sha' },
    artifactEnvelope: { identities: {
      engine: { id: 'stack-bench', version: null, sha256: 'engine' },
      calibration: { id: 'calibration', sha256: 'calibration-sha' },
      stackAdapter: { id: 'postgres', version: null, sha256: null },
    } },
    suites: { scenario, another: { selection: { checks: [{ stableKey: 'check.c' }] } } },
  };
  const expected = { backend: 'postgres', track: 'ecommerce', level: 3,
    fixtureSha256: 'fixture', recipe: { id: 'recipe', sha256: 'recipe-sha' },
    identities: structuredClone(bundle.artifactEnvelope.identities),
    selectedCheckKeys: ['check.b', 'check.a'] };

  assert.deepEqual(reusableMutationBaseline(bundle, expected), { ok: true, report: scenario });
  const fixtureMismatch = reusableMutationBaseline(bundle, { ...expected, fixtureSha256: 'other' });
  assert.equal(fixtureMismatch.ok, false);
  if (fixtureMismatch.ok) assert.fail('fixture mismatch was accepted');
  assert.match(fixtureMismatch.reason, /fixture/);
  const selectionMismatch = reusableMutationBaseline(bundle, { ...expected,
    selectedCheckKeys: ['check.a'] });
  assert.equal(selectionMismatch.ok, false);
  if (selectionMismatch.ok) assert.fail('selection mismatch was accepted');
  assert.match(selectionMismatch.reason, /0 exact scenario matches/);
  const engineMismatch = reusableMutationBaseline(bundle, { ...expected, identities: {
    ...expected.identities,
    engine: { ...expected.identities.engine, sha256: 'new-engine' },
  } });
  assert.equal(engineMismatch.ok, false);
  if (engineMismatch.ok) assert.fail('engine mismatch was accepted');
  assert.match(engineMismatch.reason, /engine/);
  const calibrationMismatch = reusableMutationBaseline(bundle, { ...expected, identities: {
    ...expected.identities,
    calibration: { ...expected.identities.calibration, sha256: 'new-calibration' },
  } });
  assert.equal(calibrationMismatch.ok, false);
  if (calibrationMismatch.ok) assert.fail('calibration mismatch was accepted');
  assert.match(calibrationMismatch.reason, /calibration/);
});

test('mutation definitions name exact criteria and contain real edits', () => {
  const valid = { ...mutation, file: 'src/app.ts', find: 'correct', replace: 'broken' };
  assert.equal(validateMutationDefinitions([valid]).ok, true);
  const invalid = validateMutationDefinitions([
    { id: 'duplicate', file: '', targets: [], find: 'same', replace: 'same' },
    { id: 'duplicate', file: 'src/app.ts', targets: ['check.b', 'check.b'], edits: [] },
  ]);
  assert.equal(invalid.ok, false);
  assert.deepEqual([...new Set(invalid.issues.map(issue => issue.kind))],
    ['bad_file', 'bad_targets', 'bad_edit', 'duplicate_id', 'missing_edits']);
});

test('one mutation may atomically edit multiple application files', () => {
  const multiFile = {
    ...mutation,
    edits: [
      { file: 'server/schema.sql', find: 'UNIQUE (item_id, account_id)', replace: '-- removed' },
      { file: 'server/src/index.ts', find: 'ON CONFLICT', replace: '/* no conflict guard */' },
    ],
  };
  assert.equal(validateMutationDefinitions([multiFile]).ok, true);
  assert.deepEqual(mutationFileEdits(multiFile).map(edit => edit.file),
    ['server/schema.sql', 'server/src/index.ts']);

  const inheritedFile = {
    ...mutation,
    file: 'client/src/App.tsx',
    edits: [{ find: 'first', replace: 'broken first' },
      { file: 'server/src/index.ts', find: 'second', replace: 'broken second' }],
  };
  assert.deepEqual(mutationFileEdits(inheritedFile).map(edit => edit.file),
    ['client/src/App.tsx', 'server/src/index.ts']);
  assert.equal(validateMutationDefinitions([{ ...multiFile,
    edits: [{ file: '', find: 'x', replace: 'y' }] }]).ok, false);
});

test('mutation scenarios may override a manifest default and are required for execution', () => {
  const valid = { ...mutation, file: 'src/app.ts', find: 'correct', replace: 'broken' };
  assert.equal(mutationScenario({ scenario: 'scenarios/base.json' }, valid), 'scenarios/base.json');
  assert.equal(mutationScenario({ scenario: 'scenarios/base.json' },
    { ...valid, scenario: 'scenarios/upgrade.json' }), 'scenarios/upgrade.json');
  assert.equal(validateMutationDefinitions([valid], { requireScenario: true }).ok, false);
  assert.equal(validateMutationDefinitions([{ ...valid, scenario: 'scenarios/upgrade.json' }],
    { requireScenario: true }).ok, true);
  const groups = groupMutationsByScenario({ scenario: 'scenarios/base.json', mutations: [
    valid, { ...valid, id: 'upgrade', scenario: 'scenarios/upgrade.json' },
  ] });
  assert.deepEqual([...groups.keys()], ['scenarios/base.json', 'scenarios/upgrade.json']);
  assert.deepEqual([...groups.values()].map(entries => entries.map(entry => entry.id)),
    [['break-b'], ['upgrade']]);
});

test('mutation scenarios resolve from the package assets after TypeScript compilation', () => {
  assert.equal(resolveMutationScenarioPath(
    'tracks/ecommerce/scenarios/01-account-create-2.4.0.json'),
  join(loadTrack('ecommerce').dir, 'scenarios', '01-account-create-2.4.0.json'));
  assert.throws(() => resolveMutationScenarioPath('dist/tracks/ecommerce/scenarios/example.json'),
    /escapes the tracks directory/);
  assert.throws(() => resolveMutationScenarioPath('../outside.json'),
    /escapes the tracks directory/);
});

test('recipe-bound mutation grading selects only checks owned by the scenario', () => {
  const track = loadTrack('ecommerce');
  const release = buildRecipeRelease(join(track.dir, 'composition', 'recipes',
    'sequential-l2.json'), { trackRoot: track.dir });
  const keys = releaseScenarioCheckKeys(release, track.dir,
    join(track.dir, 'scenarios', '02-features.json'));
  assert(keys.includes('ecommerce.operations-access.fulfilment-queue.1a'));
  assert(!keys.includes('ecommerce.operations-access.fulfilment-queue.1b'),
    '1b moved to a self-contained scenario and must not be graded from the legacy source');
  assert(!keys.includes('ecommerce.inventory-operations.operational-views.5a'),
    '5a moved to an isolated scenario and must not share state with the remaining views');
  assert.deepEqual(releaseScenarioCheckKeys(release, track.dir,
    join(track.dir, 'scenarios', '02-low-stock.json')), [
    'ecommerce.inventory-operations.operational-views.5a',
  ]);
  assert.throws(() => releaseScenarioCheckKeys(release, track.dir,
    join(track.dir, 'scenarios', '03-features.json')),
  /has no checks/);
});

test('recipe-bound mutation grading excludes zero-point controls', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 3, 'ecommerce.progression-catalog');
  assert.deepEqual(releaseScenarioCheckKeys(binding.release, track.dir,
    join(track.dir, 'scenarios', '01-restock-race.json')), [
    'ecommerce.spec.concurrency-safety.restock-race.202a',
  ]);
});

test('recipe-bound mutation grading keeps only checks selected for the run', () => {
  const track = loadTrack('ecommerce');
  const binding = resolveRecipeRelease(track, 3, 'ecommerce.progression-catalog');
  assert.deepEqual(releaseScenarioCheckKeys(binding.release, track.dir,
    join(track.dir, 'scenarios', '02-strengthened.json'), [
      'ecommerce.inventory-operations.warehouse-transfer.2a',
    ]), [
    'ecommerce.inventory-operations.warehouse-transfer.2a',
  ]);
});

test('a mutation can declare exact targets across multiple features', () => {
  const crossFeature = {
    id: 'cross-feature', file: 'src/app.ts', find: 'correct', replace: 'broken',
    targets: ['check.b', 'check.c'],
  };
  assert.equal(validateMutationDefinitions([crossFeature]).ok, true);
  assert.equal(validateMutationDefinitions([{ ...crossFeature, breaks: 7, kills: ['b'] }]).ok, false);
  const baseline = { total: 2, max: 2, features: [
    { id: 7, criteria: [report({ b: true }).features[0]!.criteria[0]!] },
    { id: 8, criteria: [report({ c: true }).features[0]!.criteria[0]!] },
  ] };
  const mutant = { total: 0, max: 2, features: [
    { id: 7, criteria: [report({ b: false }).features[0]!.criteria[0]!] },
    { id: 8, criteria: [report({ c: false }).features[0]!.criteria[0]!] },
  ] };
  assert.equal(validateMutationBaseline(baseline, [crossFeature]).ok, true);
  assert.equal(classifyMutationResult(baseline, mutant, crossFeature).status, 'CAUGHT');
});

test('mutation files cannot escape the application directory', () => {
  assert.match(resolveMutationFile('/fixture/app', 'server/src/index.ts'), /server[\\/]src[\\/]index\.ts$/);
  assert.throws(() => resolveMutationFile('/fixture/app', '../outside.ts'), /escapes the app directory/);
  assert.throws(() => resolveMutationFile('/fixture/app', '/outside.ts'), /escapes the app directory/);
});

test('a mutation baseline must pass every criterion and contain every declared target', () => {
  assert.equal(validateMutationBaseline(report({ a: true, b: true }), [mutation]).ok, true);
  const invalid = validateMutationBaseline(report({ a: true, b: 'inconclusive' }), [
    mutation, { id: 'missing', targets: ['check.c'] },
  ]);
  assert.equal(invalid.ok, false);
  assert.deepEqual(invalid.issues.map(issue => issue.kind),
    ['score_not_full', 'inconclusive', 'missing_target']);
});

test('mutation reports reject duplicate criterion identities', () => {
  const duplicate = report({ a: true });
  duplicate.features[0]!.criteria.push({ ...duplicate.features[0]!.criteria[0]! });
  assert.throws(() => indexMutationReport(duplicate), /duplicate mutation criterion identity/);
});

test('only transient baseline failures are retried', () => {
  assert.equal(isRetryableMutationBaseline([
    { kind: 'score_not_full' }, { kind: 'setup_failure' }, { kind: 'inconclusive' },
  ]), true);
  assert.equal(isRetryableMutationBaseline([
    { kind: 'score_not_full' }, { kind: 'criterion_failure' },
  ]), false);
  assert.equal(isRetryableMutationBaseline([{ kind: 'missing_target' }]), false);
});

test('only unusable mutation results are retried', () => {
  for (const status of ['INVALID_SETUP', 'INVALID_HARNESS_FAILURE',
    'INVALID_INCONCLUSIVE'] as const) {
    assert.equal(isRetryableMutationResult(status), true);
  }
  for (const status of ['CAUGHT', 'INVALID_REPORT', 'WRONG_CRITERION', 'SURVIVED',
    'CAUGHT_COLLATERAL', 'CAUGHT_OFF_ASSERTION'] as const) {
    assert.equal(isRetryableMutationResult(status), false);
  }
});

test('only a conclusive failure of the declared criterion is a clean kill', () => {
  const harnessFailure = (summary: string) => createCheckEvidence({ status: 'harness_failure',
    code: 'browser_failure', phase: 'assertion', summary, startedAtMs: 1, completedAtMs: 2 });
  const targetHarness = report({ a: true, b: true });
  targetHarness.total = 1;
  targetHarness.features[0]!.criteria[1] = { id: 'b', stableKey: 'check.b',
    evidence: harnessFailure('not an application observation') };
  const collateralHarness = report({ a: true, b: false });
  collateralHarness.features[0]!.criteria[0] = { id: 'a', stableKey: 'check.a',
    evidence: harnessFailure('browser disappeared') };
  const cases: Array<{ name: string; mutant: ReturnType<typeof report>; status: string;
    fields?: Record<string, unknown>; regressions?: string[]; collateral?: string[] }> = [
    { name: 'the declared criterion fails conclusively', mutant: report({ a: true, b: false }),
      status: 'CAUGHT', regressions: ['check.b'] },
    { name: 'the target fails at a database observation',
      mutant: report({ a: true, b: false }, null, 'dbExpectStock'), status: 'CAUGHT' },
    { name: 'the target fails before any observation',
      mutant: report({ a: true, b: false }, null, 'click'), status: 'CAUGHT_OFF_ASSERTION',
      fields: { targetOffAssertion: [{ key: 'check.b', action: 'click' }] } },
    { name: 'a score drop caused by setup failure',
      mutant: report({ a: false, b: false }, 'sign in failed'), status: 'INVALID_SETUP' },
    { name: 'an inconclusive target', mutant: report({ a: true, b: 'inconclusive' }),
      status: 'INVALID_INCONCLUSIVE' },
    { name: 'a typed harness failure on the target', mutant: targetHarness,
      status: 'INVALID_HARNESS_FAILURE', fields: { targetHarnessFailures: ['check.b'] } },
    { name: 'inconclusive evidence outside the target', mutant: report({ a: 'inconclusive', b: false }),
      status: 'INVALID_INCONCLUSIVE', fields: { collateralInconclusive: ['check.a'] } },
    { name: 'a harness failure outside the target', mutant: collateralHarness,
      status: 'INVALID_HARNESS_FAILURE', fields: { collateralHarnessFailures: ['check.a'] } },
    { name: 'failure in the wrong criterion', mutant: report({ a: false, b: true }),
      status: 'WRONG_CRITERION', collateral: ['check.a'] },
    { name: 'collateral damage beside a caught target', mutant: report({ a: false, b: false }),
      status: 'CAUGHT_COLLATERAL' },
  ];
  for (const { name, mutant, status, fields = {}, regressions, collateral } of cases) {
    const result = classifyMutationResult(report({ a: true, b: true }), mutant, mutation);
    assert.equal(result.status, status, name);
    for (const [field, value] of Object.entries(fields)) {
      assert.deepEqual((result as unknown as Record<string, unknown>)[field], value, name);
    }
    if (regressions) assert.deepEqual(result.regressions.map(item => item.key), regressions, name);
    if (collateral) assert.deepEqual(result.collateral.map(item => item.key), collateral, name);
  }
});
