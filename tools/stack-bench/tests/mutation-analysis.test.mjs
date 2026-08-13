import assert from 'node:assert/strict';
import test from 'node:test';
import { classifyMutationResult, groupMutationsByScenario, mutationScenario,
  validateMutationBaseline, resolveMutationFile,
  validateMutationDefinitions } from '../mutation-analysis.mjs';
import { createCheckEvidence } from '../check-evidence.mjs';

const report = (criteria, setupError = null) => ({
  total: Object.values(criteria).filter(value => value === true).length,
  max: Object.keys(criteria).length,
  features: [{
    id: 7,
    criteria: Object.entries(criteria).map(([id, value]) => ({ id,
      evidence: createCheckEvidence({
        status: value === true ? 'passed' : value === 'inconclusive' ? 'inconclusive' : 'failed',
        code: value === true ? 'completed' : 'test_result',
        phase: setupError ? 'setup' : 'assertion', summary: setupError,
        startedAtMs: 1, completedAtMs: 2,
      }) })),
  }],
});
const mutation = { id: 'break-b', breaks: 7, kills: ['b'] };

test('mutation definitions name exact criteria and contain real edits', () => {
  const valid = { ...mutation, file: 'src/app.ts', find: 'correct', replace: 'broken' };
  assert.equal(validateMutationDefinitions([valid]).ok, true);
  const invalid = validateMutationDefinitions([
    { id: 'duplicate', file: '', breaks: '7', kills: [], find: 'same', replace: 'same' },
    { id: 'duplicate', file: 'src/app.ts', breaks: 7, kills: ['b', 'b'], edits: [] },
  ]);
  assert.equal(invalid.ok, false);
  assert.deepEqual([...new Set(invalid.issues.map(issue => issue.kind))],
    ['bad_file', 'bad_feature', 'bad_kills', 'bad_edit', 'duplicate_id', 'missing_edits']);
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

test('a mutation can declare exact targets across multiple features', () => {
  const crossFeature = {
    id: 'cross-feature', file: 'src/app.ts', find: 'correct', replace: 'broken',
    targets: [{ feature: 7, criterion: 'b' }, { feature: 8, criterion: 'c' }],
  };
  assert.equal(validateMutationDefinitions([crossFeature]).ok, true);
  assert.equal(validateMutationDefinitions([{ ...crossFeature, breaks: 7, kills: ['b'] }]).ok, false);
  const baseline = { total: 2, max: 2, features: [
    { id: 7, criteria: [report({ b: true }).features[0].criteria[0]] },
    { id: 8, criteria: [report({ c: true }).features[0].criteria[0]] },
  ] };
  const mutant = { total: 0, max: 2, features: [
    { id: 7, criteria: [report({ b: false }).features[0].criteria[0]] },
    { id: 8, criteria: [report({ c: false }).features[0].criteria[0]] },
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
    mutation, { id: 'missing', breaks: 7, kills: ['c'] },
  ]);
  assert.equal(invalid.ok, false);
  assert.deepEqual(invalid.issues.map(issue => issue.kind),
    ['score_not_full', 'inconclusive', 'missing_target']);
});

test('only a conclusive failure of the declared criterion is a clean kill', () => {
  const result = classifyMutationResult(report({ a: true, b: true }), report({ a: true, b: false }), mutation);
  assert.equal(result.status, 'CAUGHT');
  assert.deepEqual(result.regressions.map(item => item.key), ['7:b']);
});

test('a score drop caused by setup failure is rejected', () => {
  const result = classifyMutationResult(report({ a: true, b: true }),
    report({ a: false, b: false }, 'sign in failed'), mutation);
  assert.equal(result.status, 'INVALID_SETUP');
});

test('an inconclusive target does not kill a mutant', () => {
  const result = classifyMutationResult(report({ a: true, b: true }),
    report({ a: true, b: 'inconclusive' }), mutation);
  assert.equal(result.status, 'INVALID_INCONCLUSIVE');
});

test('a typed harness failure is not mistaken for an inconclusive or caught mutant', () => {
  const evidence = createCheckEvidence({ status: 'harness_failure', code: 'browser_failure',
    phase: 'assertion', summary: 'not an application observation', startedAtMs: 1, completedAtMs: 2 });
  const mutant = report({ a: true, b: true });
  mutant.total = 1;
  mutant.features[0].criteria[1] = { id: 'b', evidence };
  const result = classifyMutationResult(report({ a: true, b: true }), mutant, mutation);
  assert.equal(result.status, 'INVALID_HARNESS_FAILURE');
  assert.deepEqual(result.targetHarnessFailures, ['7:b']);
});

test('failure in the wrong criterion is distinguished from survival', () => {
  const result = classifyMutationResult(report({ a: true, b: true }), report({ a: false, b: true }), mutation);
  assert.equal(result.status, 'WRONG_CRITERION');
  assert.deepEqual(result.collateral.map(item => item.key), ['7:a']);
});

test('collateral damage fails even when the intended criterion catches the mutant', () => {
  const result = classifyMutationResult(report({ a: true, b: true }), report({ a: false, b: false }), mutation);
  assert.equal(result.status, 'CAUGHT_COLLATERAL');
});
