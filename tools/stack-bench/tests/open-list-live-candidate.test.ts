import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { mutationEdits, mutationScenario, mutationTargetKeys,
  readMutationManifest, validateMutationDefinitions } from '../src/evidence/mutation-analysis.js';
import { loadReferenceRegistry, prepareReferenceFixtureSource,
  selectReferenceFixture } from '../src/references/reference-fixtures.js';
import type { MutationDefinition } from '../src/evidence/mutation-analysis.js';
import type { ReferenceFixtureSelector } from '../src/references/reference-fixtures.js';

const ROOT = STACK_BENCH_ROOT;
const SCENARIO_RELATIVE = 'tracks/ecommerce/scenarios/01-open-list-live-2.3.0.json';
const MUTATION_SCENARIO = 'tracks/ecommerce/scenarios/progression-open-list-live-1.0.0.json';
const SCENARIO = join(ROOT, SCENARIO_RELATIVE);
const registry = loadReferenceRegistry();

const cases: ReadonlyArray<readonly [backend: string, fixtureSha256: string]> = [
  ['mongodb', 'e183f9581446ed52c36d422e79298f933eb57b973591f5473da687008161428e'],
  ['postgres', '2c6d23e255d1ca930dd9fd8af1290dcfe0207cc0a7c9feefd9ec40889b0520bf'],
  ['spacetime', '66ebf11ff476e23909ec7f972e509eb89844c42ba70ad91b6051d5078dd2e101'],
];

interface ReferenceSourceArguments extends ReferenceFixtureSelector {
  app: string;
}

function prepareReferenceSource(args: ReferenceSourceArguments) {
  const fixture = selectReferenceFixture(loadReferenceRegistry(), args);
  const prepared = prepareReferenceFixtureSource(fixture, args.app);
  return { fixture, sourceSha256: prepared.sha256 };
}

test('the focused 902a candidate deterministically checks an already-open live list', () => {
  const scenario = compileScenarioDefinition(readJson(SCENARIO),
    { source: SCENARIO, expectedLevel: 1 });
  const feature = scenario.features.find(candidate => candidate.id === 902);
  assert(feature, 'scenario must include feature 902');
  const criterion = feature.criteria.find(candidate => String(candidate.id) === '902a');
  assert(criterion, 'feature 902 must include criterion 902a');
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
  assert.match(stringValue(criterion.note, 'criterion 902a note'),
    /without assuming HTTP, WebSockets, subscriptions, or any project layout/);
});

for (const [backend, fixtureSha256] of cases) {
  test(`${backend} has exact known defects for the open-list check`, () => {
    const fixture = registry.fixtures.find(candidate =>
      candidate.track === 'ecommerce' && candidate.backend === backend);
    assert.equal(fixture?.mutationManifests?.length, 1);
    const manifestPath = fixture?.mutationManifests?.[0];
    assert(manifestPath);
    const manifest = readMutationManifest(join(ROOT, manifestPath));
    const mutations = manifest.mutations.filter(mutation =>
      mutationTargetKeys(mutation).includes('ecommerce.spec.live-state.open-list.902a'));
    assert.equal(manifest.fixtureSha256, fixtureSha256);
    assert.deepEqual(validateMutationDefinitions(mutations, {
      defaultScenario: manifest.scenario,
      requireScenario: true,
    }).issues, []);
    assert.equal(mutations.length, backend === 'mongodb' ? 1 : 2);
    assert(mutations.every(mutation => mutationTargetKeys(mutation).length === 1
      && mutationTargetKeys(mutation)[0] === 'ecommerce.spec.live-state.open-list.902a'));
    assert(mutations.some(mutation => /ignore|snapshot/i.test(mutationDescription(mutation))),
      'one mutation must omit the committed review from the open reader');
    if (backend !== 'mongodb') {
      assert(mutations.some(mutation => /twice/i.test(mutationDescription(mutation))),
        'one mutation must duplicate the committed review');
    }

    const work = mkdtempSync(join(tmpdir(), `stack-bench-open-list-${backend}-`));
    try {
      const app = join(work, 'app');
      const prepared = prepareReferenceSource({
        backend,
        track: 'ecommerce',
        level: 1,
        recipe: 'ecommerce.sequential-l1@2.5.0',
        app,
      });
      assert.equal(prepared.sourceSha256, fixtureSha256);
      for (const mutation of mutations) {
        assert.equal(mutationScenario(manifest, mutation), MUTATION_SCENARIO);
        const source = readFileSync(join(app, mutationFile(mutation)), 'utf8');
        let mutated = source;
        for (const edit of validMutationEdits(mutation)) {
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
          fileName: mutationFile(mutation),
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

function readJson(path: string): unknown {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function stringValue(value: unknown, label: string): string {
  if (typeof value !== 'string') throw new Error(`${label} must be a string`);
  return value;
}

function mutationDescription(mutation: MutationDefinition): string {
  return stringValue(mutation.desc, 'mutation description');
}

function mutationFile(mutation: MutationDefinition): string {
  return stringValue(mutation.file, 'mutation file');
}

function validMutationEdits(mutation: MutationDefinition): Array<{ find: string; replace: string }> {
  return mutationEdits(mutation).map(edit => ({
    find: stringValue(edit.find, 'mutation edit find'),
    replace: stringValue(edit.replace, 'mutation edit replace'),
  }));
}
