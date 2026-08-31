import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { mutationScenario, mutationTargetKeys,
  readMutationManifest, validateMutationDefinitions } from '../src/evidence/mutation-analysis.js';
import { loadReferenceRegistry, prepareReferenceFixtureSource,
  selectReferenceFixture } from '../src/references/reference-fixtures.js';

const ROOT = STACK_BENCH_ROOT;
const scenarioPath = 'tracks/ecommerce/scenarios/01-duplicate-checkout-2.3.1.json';
const cases = [
  {
    backend: 'mongodb',
    fixtureSha256: '24d445f18cdcb25b9ab06dd4f4582003b348edaf0be0b97c27ec9fbb06751b1e',
    manifest: 'mongodb-ecommerce-2.0.1.json',
    mutationId: 'concurrent-cart-add-does-not-increment',
  },
  {
    backend: 'postgres',
    fixtureSha256: 'f8f6152cebd68acc2af46a3e4cbf208664db17a7db58e0276d8e10ea6750aaf5',
    manifest: 'postgres-ecommerce-2.0.1.json',
    mutationId: 'progression-concurrent-cart-line-does-not-increment',
  },
  {
    backend: 'spacetime',
    fixtureSha256: '8806e01bcd4d44fa7c7c491f722c2412d568605a9d666545dafcc0bdf2a2b4f5',
    manifest: 'spacetime-ecommerce-2.0.1.json',
    mutationId: 'existing-cart-line-does-not-increment',
  },
];

function prepareReferenceSource(args: { backend: string; track: string; level: number;
  recipe: string; app: string }) {
  const fixture = selectReferenceFixture(loadReferenceRegistry(), args);
  const prepared = prepareReferenceFixtureSource(fixture, args.app);
  return { fixture, sourceSha256: prepared.sha256 };
}

function load(entry: typeof cases[number]) {
  return readMutationManifest(join(ROOT, 'grader', 'mutations', entry.manifest));
}

test('the 203a mutation scenario compiles and owns the concurrent quantity assertion', () => {
  const scenario = compileScenarioDefinition(
    JSON.parse(readFileSync(join(ROOT, scenarioPath), 'utf8')),
    { source: scenarioPath, expectedLevel: 1 },
  );
  const feature = scenario.features.find(candidate => candidate.id === 203);
  assert.ok(feature);
  const criterion = feature.criteria.find(candidate => candidate.id === '203a');
  assert.ok(criterion);
  assert.deepEqual(criterion.steps.map(step => step.do), ['expect', 'expectNumber']);
  const secondStep = criterion.steps[1];
  assert.ok(secondStep);
  assert.equal(secondStep.equals, 2);
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
        recipe: 'ecommerce.sequential-l1@2.5.0',
        app,
      });
      assert.equal(prepared.fixture.id, `ecommerce-reference-${entry.backend}`);
      assert.equal(prepared.sourceSha256, entry.fixtureSha256);
      assert.equal(manifest.schemaVersion, 2);
      assert.equal(manifest.status, 'active');
      assert.equal(manifest.backend, entry.backend);
      assert.equal(manifest.track, 'ecommerce');
      assert.equal(manifest.level, undefined);
      assert.equal(manifest.fixtureSha256, prepared.sourceSha256);
      assert.deepEqual(validateMutationDefinitions(manifest.mutations, {
        defaultScenario: manifest.scenario,
        requireScenario: true,
      }).issues, []);
      const mutation = manifest.mutations.find(candidate =>
        candidate.id === entry.mutationId);
      assert.ok(mutation);
      assert.equal(mutationScenario(manifest, mutation), scenarioPath);
      assert.equal(mutation.id, entry.mutationId);
      assert.deepEqual(mutationTargetKeys(mutation), [
        'ecommerce.spec.concurrency-safety.duplicate-checkout.203a',
      ]);
      assert.equal(mutation.edits.length, 1);

      assert.ok(mutation.file);
      const source = readFileSync(join(app, mutation.file), 'utf8');
      for (const edit of mutation.edits) {
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
