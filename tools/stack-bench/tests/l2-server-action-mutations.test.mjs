import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { compileScenarioDefinition } from '../definition-compiler.mjs';
import { mutationEdits, mutationScenario, mutationTargetKeys,
  validateMutationDefinitions } from '../mutation-analysis.mjs';
import { prepareReferenceSource } from '../reference-agent.mjs';

const ROOT = join(import.meta.dirname, '..');
const focusedScenario = 'tracks/ecommerce/scenarios/02-server-actions-1.0.0.json';
const inheritedShipScenario = 'tracks/ecommerce/scenarios/02-features.json';
const cases = [
  {
    backend: 'mongodb',
    fixtureId: 'ecommerce-l2-server-actions-mongodb',
    fixtureSha256: 'b145b0ac453f7d51d1fe86463b2393bdda92f82d38d97b950ab347bf8587980d',
    manifest: 'mongodb-ecom-l2-cumulative-1.3.0.json',
    lastUnitMutation: 'oversell-unguarded-decrement',
    mutationIds: ['oversell-unguarded-decrement', 'existing-cart-line-does-not-increment',
      'checkout-not-idempotent', 'customer-can-ship-order-replay',
      'customer-can-ship-order-direct',
      'customer-can-cancel-foreign-order'],
  },
  {
    backend: 'postgres',
    fixtureId: 'ecommerce-l2-server-actions-postgres',
    fixtureSha256: '574bea4e918b7ec15eb3a182e68b45bfe2630a07fee4ac4cf06b57268dd6add1',
    manifest: 'postgres-ecom-l2-cumulative-1.3.0.json',
    lastUnitMutation: 'oversell-no-row-lock',
    mutationIds: ['oversell-no-row-lock', 'existing-cart-line-does-not-increment',
      'checkout-does-not-empty-cart', 'customer-can-ship-order-replay',
      'customer-can-ship-order-direct',
      'customer-can-cancel-foreign-order'],
  },
  {
    backend: 'spacetime',
    fixtureId: 'ecommerce-l2-server-actions-spacetime',
    fixtureSha256: '9acf6f1223daef25dc855e29211b5db0116fd0d431f5185d264a0c48db5152f1',
    manifest: 'spacetime-ecom-l2-cumulative-1.3.0.json',
    lastUnitMutation: 'purchase-does-not-reserve-stock-last-unit',
    mutationIds: ['purchase-does-not-reserve-stock-last-unit',
      'purchase-does-not-reserve-stock-restock-race',
      'existing-cart-line-does-not-increment', 'checkout-does-not-empty-cart',
      'customer-can-ship-order-replay', 'customer-can-ship-order-direct',
      'customer-can-cancel-foreign-order'],
  },
];

function load(entry) {
  return JSON.parse(readFileSync(join(ROOT, 'grader', 'mutations', 'candidates', entry.manifest),
    'utf8'));
}

test('the focused L2 scenario scores all three direct server checks', () => {
  const scenario = compileScenarioDefinition(
    JSON.parse(readFileSync(join(ROOT, focusedScenario), 'utf8')),
    { source: focusedScenario, expectedLevel: 2 },
  );
  assert.deepEqual(scenario.features.flatMap(feature => feature.criteria.map(criterion => ({
    key: `${feature.id}:${criterion.id}`,
    points: criterion.points,
  }))), [
    { key: '201:201c', points: 2 },
    { key: '202:202d', points: 2 },
    { key: '204:204a', points: 2 },
  ]);
});

for (const entry of cases) {
  test(`${entry.backend} L2 mutations are exact and bound to the derived 1.3 source`, () => {
    const work = mkdtempSync(join(tmpdir(), `stack-bench-l2-actions-${entry.backend}-`));
    try {
      const manifest = load(entry);
      const app = join(work, 'app');
      const prepared = prepareReferenceSource({
        backend: entry.backend,
        track: 'ecommerce',
        level: 2,
        recipe: 'ecommerce.l2-standard@1.3.0',
        app,
      });
      assert.equal(prepared.fixture.id, entry.fixtureId);
      assert.equal(prepared.sourceSha256, entry.fixtureSha256);
      assert.equal(manifest.schemaVersion, 1);
      assert.equal(manifest.status, 'candidate');
      assert.equal(manifest.backend, entry.backend);
      assert.equal(manifest.track, 'ecommerce');
      assert.equal(manifest.level, 2);
      assert.equal(manifest.fixtureSha256, prepared.sourceSha256);
      assert.deepEqual(validateMutationDefinitions(manifest.mutations, {
        defaultScenario: manifest.scenario,
        requireScenario: true,
      }).issues, []);
      assert.deepEqual(manifest.mutations.map(mutation => mutation.id), entry.mutationIds);

      const lastUnit = manifest.mutations.find(mutation => mutation.id === entry.lastUnitMutation);
      assert.deepEqual(mutationTargetKeys(lastUnit), ['201:201a', '201:201b', '201:201c']);
      const inheritedShip = manifest.mutations.find(mutation =>
        mutation.id === 'customer-can-ship-order-replay');
      assert.equal(mutationScenario(manifest, inheritedShip), inheritedShipScenario);
      assert.deepEqual(mutationTargetKeys(inheritedShip), ['1:1e']);
      const ship = manifest.mutations.find(mutation => mutation.id === 'customer-can-ship-order-direct');
      assert.equal(mutationScenario(manifest, ship), focusedScenario);
      assert.deepEqual(mutationTargetKeys(ship), ['201:201c']);
      const cancel = manifest.mutations.find(mutation =>
        mutation.id === 'customer-can-cancel-foreign-order');
      assert.equal(mutationScenario(manifest, cancel), focusedScenario);
      assert.deepEqual(mutationTargetKeys(cancel), ['204:204a']);
      assert.equal(manifest.mutations.some(mutation =>
        mutationTargetKeys(mutation).includes('202:202d')), false,
      '202d must not claim a mutant until deterministic interleaving exists');

      for (const mutation of manifest.mutations) {
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
      }
    } finally {
      rmSync(work, { recursive: true, force: true });
    }
  });
}
