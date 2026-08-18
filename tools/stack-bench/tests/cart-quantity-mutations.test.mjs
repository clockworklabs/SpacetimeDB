import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { compileScenarioDefinition } from '../src/composition/definition-compiler.mjs';
import { mutationEdits, mutationScenario, mutationTargetKeys,
  validateMutationDefinitions } from '../src/evidence/mutation-analysis.mjs';
import { prepareReferenceSource } from '../src/references/reference-agent.mjs';

const ROOT = join(import.meta.dirname, '..');
const scenarioPath = 'tracks/ecommerce/scenarios/01-duplicate-checkout-2.3.0.json';
const cases = [
  {
    backend: 'mongodb',
    fixtureSha256: 'd90ea9c8326202a76bf570d0eb7c716531e3e6e3eb4a4678c677783e9d5dbb40',
    manifest: 'mongodb-ecom-l1-modular-2.3.0.json',
  },
  {
    backend: 'postgres',
    fixtureSha256: 'ffc2192ee7bce1a5f5e60bd4158118f44dd0d5cc1fcf0bcf21bc38fbfb20d6f1',
    manifest: 'postgres-ecom-l1-modular-2.3.0.json',
  },
  {
    backend: 'spacetime',
    fixtureSha256: '7deedf0dc4c17064b9a6a9bb76bc0c488cd04f21472ce7412539ac98368fd3e6',
    manifest: 'spacetime-ecom-l1-modular-2.3.0.json',
  },
];

function load(entry) {
  return JSON.parse(readFileSync(join(ROOT, 'grader', 'mutations', entry.manifest), 'utf8'));
}

test('the 203a mutation scenario compiles and owns the concurrent quantity assertion', () => {
  const scenario = compileScenarioDefinition(
    JSON.parse(readFileSync(join(ROOT, scenarioPath), 'utf8')),
    { source: scenarioPath, expectedLevel: 1 },
  );
  const feature = scenario.features.find(candidate => candidate.id === 203);
  const criterion = feature.criteria.find(candidate => candidate.id === '203a');
  assert.deepEqual(criterion.steps.map(step => step.do), ['expect', 'expectNumber']);
  assert.equal(criterion.steps[1].equals, 2);
});

for (const entry of cases) {
  test(`${entry.backend} has one exact, source-bound 203a increment defect`, () => {
    const work = mkdtempSync(join(tmpdir(), `stack-bench-203a-${entry.backend}-`));
    try {
      const manifest = load(entry);
      const app = join(work, 'app');
      const prepared = prepareReferenceSource({
        backend: entry.backend,
        track: 'ecommerce',
        level: 1,
        recipe: 'ecommerce.l1-modular@2.3.0',
        app,
      });
      assert.equal(prepared.fixture.id, `ecommerce-l1-direct-actions-${entry.backend}`);
      assert.equal(prepared.sourceSha256, entry.fixtureSha256);
      assert.equal(manifest.schemaVersion, 1);
      assert.equal(manifest.status, 'active');
      assert.equal(manifest.backend, entry.backend);
      assert.equal(manifest.track, 'ecommerce');
      assert.equal(manifest.level, 1);
      assert.equal(manifest.fixtureSha256, prepared.sourceSha256);
      assert.deepEqual(validateMutationDefinitions(manifest.mutations, {
        defaultScenario: manifest.scenario,
        requireScenario: true,
      }).issues, []);
      const mutation = manifest.mutations.find(candidate =>
        candidate.id === 'existing-cart-line-does-not-increment');
      assert.equal(mutationScenario(manifest, mutation), scenarioPath);
      assert.equal(mutation.id, 'existing-cart-line-does-not-increment');
      assert.deepEqual(mutationTargetKeys(mutation), ['203:203a']);
      assert.equal(mutationEdits(mutation).length, 1);

      const source = readFileSync(join(app, mutation.file), 'utf8');
      for (const edit of mutationEdits(mutation)) {
          assert.equal(source.split(edit.find).length - 1, 1,
            `${mutation.id} anchor must match exactly once`);
          const mutated = source.replace(edit.find, edit.replace);
          assert.equal(mutated.includes(edit.find), false,
            `${mutation.id} must remove its exact correct behavior`);
          assert.equal(mutated.includes(edit.replace), true,
            `${mutation.id} must install its declared defect`);
          const transpiled = ts.transpileModule(mutated, {
            compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
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
