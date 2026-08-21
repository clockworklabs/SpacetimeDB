import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { compileScenarioDefinition } from '../src/composition/definition-compiler.mjs';
import { mutationEdits, mutationScenario, mutationTargetKeys,
  validateMutationDefinitions } from '../src/evidence/mutation-analysis.mjs';
import { loadReferenceRegistry, prepareReferenceFixtureSource,
  selectReferenceFixture } from '../src/references/reference-fixtures.mjs';

const ROOT = join(import.meta.dirname, '..');
const SCENARIO_RELATIVE = 'tracks/ecommerce/scenarios/01-open-list-live-2.3.0.json';
const SCENARIO = join(ROOT, SCENARIO_RELATIVE);
const MUTATIONS = join(ROOT, 'grader', 'mutations');

const cases = [
  ['mongodb', 'd90ea9c8326202a76bf570d0eb7c716531e3e6e3eb4a4678c677783e9d5dbb40'],
  ['postgres', 'ffc2192ee7bce1a5f5e60bd4158118f44dd0d5cc1fcf0bcf21bc38fbfb20d6f1'],
  ['spacetime', '7deedf0dc4c17064b9a6a9bb76bc0c488cd04f21472ce7412539ac98368fd3e6'],
];

function prepareReferenceSource(args) {
  const fixture = selectReferenceFixture(loadReferenceRegistry(), args);
  const prepared = prepareReferenceFixtureSource(fixture, args.app);
  return { fixture, sourceSha256: prepared.sha256 };
}

test('the focused 902a candidate deterministically checks an already-open live list', () => {
  const scenario = compileScenarioDefinition(JSON.parse(readFileSync(SCENARIO, 'utf8')),
    { source: SCENARIO, expectedLevel: 1 });
  const feature = scenario.features.find(candidate => candidate.id === 902);
  const criterion = feature.criteria.find(candidate => String(candidate.id) === '902a');
  assert.equal(criterion.points, 1);
  assert.deepEqual(criterion.steps.map(step => step.do),
    ['click', 'expect', 'fill', 'click', 'expectElementCount', 'expectElementCount']);
  assert.deepEqual(criterion.steps.slice(0, 2).map(step => step.actor), ['reader', 'reader'],
    'the reader view must be visibly open before the write begins');
  assert.equal(criterion.steps.some(action => action.do === 'race' || action.do === 'wait'), false);
  assert.deepEqual(criterion.steps.at(-2), {
    do: 'expectElementCount', actor: 'reviewer', testid: 'review-item',
    contains: 'live-review-kbd', equals: 1, within: 10000,
  }, 'the writer must prove the review committed before grading the reader');
  assert.deepEqual(criterion.steps.at(-1), {
    do: 'expectElementCount', actor: 'reader', testid: 'review-item',
    contains: 'live-review-kbd', equals: 1, within: 10000,
  });
  assert.match(criterion.note, /without assuming HTTP, WebSockets, subscriptions, or any project layout/);
});

for (const [backend, fixtureSha256] of cases) {
  test(`${backend} has exact omission and duplication defects for the open-list check`, () => {
    const manifest = JSON.parse(readFileSync(join(MUTATIONS,
      `${backend}-ecom-l1-modular-2.3.0.json`), 'utf8'));
    const mutations = manifest.mutations.filter(mutation =>
      mutationTargetKeys(mutation).includes('902:902a'));
    assert.equal(manifest.fixtureSha256, fixtureSha256);
    assert.deepEqual(validateMutationDefinitions(mutations, {
      defaultScenario: manifest.scenario,
      requireScenario: true,
    }).issues, []);
    assert.equal(mutations.length, 2);
    assert.deepEqual(mutations.map(mutation => mutationTargetKeys(mutation)),
      [[`902:902a`], [`902:902a`]]);
    assert(mutations.some(mutation => /ignore|snapshot/i.test(mutation.desc)),
      'one mutation must omit the committed review from the open reader');
    assert(mutations.some(mutation => /twice/i.test(mutation.desc)),
      'one mutation must duplicate the committed review');

    const work = mkdtempSync(join(tmpdir(), `stack-bench-open-list-${backend}-`));
    try {
      const app = join(work, 'app');
      const prepared = prepareReferenceSource({
        backend,
        track: 'ecommerce',
        level: 1,
        recipe: 'ecommerce.l1-modular@2.3.0',
        app,
      });
      assert.equal(prepared.sourceSha256, fixtureSha256);
      for (const mutation of mutations) {
        assert.equal(mutationScenario(manifest, mutation), SCENARIO_RELATIVE);
        const source = readFileSync(join(app, mutation.file), 'utf8');
        let mutated = source;
        for (const edit of mutationEdits(mutation)) {
          assert.equal(mutated.split(edit.find).length - 1, 1,
            `${mutation.id} anchor must match exactly once`);
          mutated = mutated.replace(edit.find, edit.replace);
        }
        const transpiled = ts.transpileModule(mutated, {
          compilerOptions: {
            jsx: ts.JsxEmit.ReactJSX,
            module: ts.ModuleKind.ESNext,
            target: ts.ScriptTarget.ES2022,
          },
          fileName: mutation.file,
          reportDiagnostics: true,
        });
        assert.deepEqual((transpiled.diagnostics ?? [])
          .filter(diagnostic => diagnostic.category === ts.DiagnosticCategory.Error)
          .map(diagnostic => ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')), []);
      }
    } finally {
      rmSync(work, { recursive: true, force: true });
    }
  });
}
