import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { mutationEdits, mutationScenario, mutationTargetKeys,
  validateMutationDefinitions } from '../mutation-analysis.mjs';
import { prepareReferenceSource } from '../reference-agent.mjs';
import { buildRecipeRelease } from '../recipe-release.mjs';
import { selectScenarioChecks } from '../recipe-selection.mjs';

const ROOT = join(import.meta.dirname, '..');
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const previous = buildRecipeRelease(join(TRACK, 'composition', 'recipes',
  'l1-modular-2.2.0.json'));
const release = buildRecipeRelease(join(TRACK, 'composition', 'recipes',
  'l1-modular-2.3.0.json'));

const LAST_UNIT = 'tracks/ecommerce/scenarios/01-last-unit-2.3.0.json';
const RESTOCK_RACE = 'tracks/ecommerce/scenarios/01-restock-race-2.3.0.json';
const DUPLICATE_CHECKOUT = 'tracks/ecommerce/scenarios/01-duplicate-checkout-2.3.0.json';
const EXTERNAL_LIVE = 'tracks/ecommerce/scenarios/01-external-live-sync-1.1.0.json';
const EXTERNAL_RECONNECT = 'tracks/ecommerce/scenarios/01-external-reconnect-sync-1.1.0.json';
const OPEN_LIST = 'tracks/ecommerce/scenarios/01-open-list-live-2.3.0.json';

const cases = [
  {
    backend: 'mongodb',
    sourceSha256: 'd90ea9c8326202a76bf570d0eb7c716531e3e6e3eb4a4678c677783e9d5dbb40',
    manifest: 'mongodb-ecom-l1-modular-2.3.0.json',
    lastUnitMutation: 'oversell-unguarded-decrement',
    expectedMutations: [
      ['oversell-unguarded-decrement', LAST_UNIT, ['201:201a', '201:201b', '201:201c']],
      ['purchase-does-not-reserve-stock-restock-race', RESTOCK_RACE, ['202:202a']],
      ['restock-does-not-increase-stock', RESTOCK_RACE, ['202:202-control', '202:202a']],
      ['existing-cart-line-does-not-increment', DUPLICATE_CHECKOUT, ['203:203a']],
      ['checkout-not-idempotent', DUPLICATE_CHECKOUT, ['203:203b']],
      ['external-stock-polling-disabled', EXTERNAL_LIVE, ['901:901a']],
      ['reconnect-generation-ignores-current-catalog', EXTERNAL_RECONNECT, ['901:901d']],
      ['open-review-list-ignores-live-update', OPEN_LIST, ['902:902a']],
      ['open-review-list-renders-each-review-twice', OPEN_LIST, ['902:902a']],
    ],
  },
  {
    backend: 'postgres',
    sourceSha256: 'ffc2192ee7bce1a5f5e60bd4158118f44dd0d5cc1fcf0bcf21bc38fbfb20d6f1',
    manifest: 'postgres-ecom-l1-modular-2.3.0.json',
    lastUnitMutation: 'oversell-no-row-lock',
    expectedMutations: [
      ['oversell-no-row-lock', LAST_UNIT, ['201:201a', '201:201b', '201:201c']],
      ['purchase-does-not-reserve-stock-restock-race', RESTOCK_RACE, ['202:202a']],
      ['restock-does-not-increase-stock', RESTOCK_RACE, ['202:202-control', '202:202a']],
      ['existing-cart-line-does-not-increment', DUPLICATE_CHECKOUT, ['203:203a']],
      ['checkout-does-not-empty-cart', DUPLICATE_CHECKOUT, ['203:203b']],
      ['external-stock-polling-disabled', EXTERNAL_LIVE, ['901:901a']],
      ['reconnect-does-not-send-current-catalog', EXTERNAL_RECONNECT, ['901:901d']],
      ['open-review-list-ignores-live-update', OPEN_LIST, ['902:902a']],
      ['open-review-list-renders-each-review-twice', OPEN_LIST, ['902:902a']],
    ],
  },
  {
    backend: 'spacetime',
    sourceSha256: '7deedf0dc4c17064b9a6a9bb76bc0c488cd04f21472ce7412539ac98368fd3e6',
    manifest: 'spacetime-ecom-l1-modular-2.3.0.json',
    lastUnitMutation: 'purchase-does-not-reserve-stock-last-unit',
    expectedMutations: [
      ['purchase-does-not-reserve-stock-last-unit', LAST_UNIT,
        ['201:201a', '201:201b', '201:201c']],
      ['purchase-does-not-reserve-stock-restock-race', RESTOCK_RACE, ['202:202a']],
      ['restock-does-not-increase-stock', RESTOCK_RACE, ['202:202-control', '202:202a']],
      ['existing-cart-line-does-not-increment', DUPLICATE_CHECKOUT, ['203:203a']],
      ['checkout-does-not-empty-cart', DUPLICATE_CHECKOUT, ['203:203b']],
      ['stock-subscription-snapshotted-once', EXTERNAL_LIVE, ['901:901a']],
      ['stock-view-keeps-pre-reconnect-snapshot', EXTERNAL_RECONNECT, ['901:901d']],
      ['open-review-list-snapshots-on-selection', OPEN_LIST, ['902:902a']],
      ['open-review-list-renders-each-review-twice', OPEN_LIST, ['902:902a']],
    ],
  },
];

function byStableKey(candidate) {
  return new Map(candidate.checkCatalog.map(check => [check.stableKey, check]));
}

test('L1 modular 2.3 changes only the six justified concurrency and live-state scores', () => {
  const before = byStableKey(previous);
  const after = byStableKey(release);
  assert.deepEqual([...after.keys()].sort(), [...before.keys()].sort());
  assert.equal(release.checkCatalog.length, 48);
  assert.equal(release.scoring.points, 58);
  assert.equal(release.checkCatalog.filter(check => check.points === 0).length, 2);

  const changed = [...after].filter(([key, check]) => check.points !== before.get(key).points)
    .map(([key, check]) => [key, before.get(key).points, check.points]);
  assert.deepEqual(changed, [
    ['ecommerce.spec.concurrency-safety.last-unit.201c', 0, 1],
    ['ecommerce.spec.concurrency-safety.restock-race.202a', 0, 1],
    ['ecommerce.spec.concurrency-safety.duplicate-checkout.203a', 0, 1],
    ['ecommerce.spec.external-data-sync.external-stock.901a', 0, 1],
    ['ecommerce.spec.external-data-sync.external-stock.901d', 0, 1],
    ['ecommerce.spec.live-state.open-list.902a', 0, 1],
  ]);
  assert.equal(after.get('ecommerce.spec.concurrency-safety.restock-race.202-control').points, 0,
    'the ordinary-restock precondition must not double-count already-scored restock behavior');
});

test('each last-unit score retains the shared purchase race when selected alone', () => {
  const scenario = JSON.parse(readFileSync(join(TRACK, 'scenarios',
    '01-last-unit-2.3.0.json'), 'utf8'));
  for (const criterionId of ['201a', '201b', '201c']) {
    const key = `ecommerce.spec.concurrency-safety.last-unit.${criterionId}`;
    const selected = selectScenarioChecks(scenario, { checks: release.checkCatalog }, [key]);
    const [feature] = selected.features;
    assert.deepEqual(feature.criteria.map(criterion => criterion.id), [criterionId]);
    const races = feature.setup.filter(step => step.do === 'clickConcurrently');
    assert.equal(races.length, 1);
    assert.deepEqual(races[0].actors, ['a', 'b', 'c', 'd', 'e', 'f']);
    assert.equal(races[0].testid, 'buy-now');
    assert.equal(races[0].in.contains, 'Mirrorless Camera');
    assert.equal(feature.criteria[0].steps.some(step => step.do === 'clickConcurrently'
      && step.testid === 'buy-now'), false);
    const baseline = feature.setup.findIndex(step => step.as === 'revenue-before-last-unit');
    assert(baseline >= 0 && baseline < feature.setup.indexOf(races[0]),
      'the revenue baseline must be recorded before the shared purchase race');
  }
});

test('the restock race retains its admin page prerequisite when selected alone', () => {
  const scenario = JSON.parse(readFileSync(join(TRACK, 'scenarios',
    '01-restock-race-2.3.0.json'), 'utf8'));
  const key = 'ecommerce.spec.concurrency-safety.restock-race.202a';
  const selected = selectScenarioChecks(scenario, { checks: release.checkCatalog }, [key]);
  const [feature] = selected.features;
  assert.deepEqual(feature.criteria.map(criterion => criterion.id), ['202a']);
  assert(feature.setup.some(step => step.do === 'click' && step.actor === 'admin'
    && step.testid === 'admin-link'));
  assert(feature.criteria[0].steps.some(step => step.do === 'race'));
});

test('duplicate checkout metadata describes the current cross-stack named action', () => {
  const scenario = JSON.parse(readFileSync(join(TRACK, 'scenarios',
    '01-duplicate-checkout-2.3.0.json'), 'utf8'));
  const checkout = scenario.features[0].criteria.find(criterion => criterion.id === '203b');
  const metadata = [checkout.note, checkout.provenBy, checkout.withheld].join('\n');
  assert.match(metadata, /callConcurrently/);
  assert.match(metadata, /MongoDB/);
  assert.match(metadata, /PostgreSQL/);
  assert.match(metadata, /SpacetimeDB/);
  assert.match(metadata, /Docker mutation qualification/);
  assert.doesNotMatch(metadata, /replayConcurrently|INCONCLUSIVE|lastWrites|captured HTTP write/);
  assert.deepEqual(checkout.steps.filter(step => ['callConcurrently', 'expectCallOutcomes']
    .includes(step.do)).map(step => step.do), ['callConcurrently', 'expectCallOutcomes']);
});

for (const entry of cases) {
  test(`${entry.backend} binds the complete L1 2.3 candidate inventory to its effective source`, () => {
    const work = mkdtempSync(join(tmpdir(), `stack-bench-l1-concurrency-${entry.backend}-`));
    try {
      const app = join(work, 'app');
      const prepared = prepareReferenceSource({
        backend: entry.backend,
        track: 'ecommerce',
        level: 1,
        recipe: 'ecommerce.l1-modular@2.3.0',
        app,
      });
      assert.equal(prepared.sourceSha256, entry.sourceSha256);

      const manifest = JSON.parse(readFileSync(join(ROOT, 'grader', 'mutations', 'candidates',
        entry.manifest), 'utf8'));
      assert.equal(manifest.fixtureSha256, prepared.sourceSha256);
      assert.deepEqual(validateMutationDefinitions(manifest.mutations, {
        defaultScenario: manifest.scenario,
        requireScenario: true,
      }).issues, []);

      assert.deepEqual(manifest.mutations.map(mutation => [
        mutation.id,
        mutationScenario(manifest, mutation),
        mutationTargetKeys(mutation),
      ]), entry.expectedMutations);
      assert.equal(manifest.mutations.some(mutation => mutationTargetKeys(mutation)
        .includes('901:901b')), false, '901b has no deterministic candidate mutation');
      const mutations = new Map(manifest.mutations.map(mutation => [mutation.id, mutation]));
      const lastUnit = mutations.get(entry.lastUnitMutation);
      assert.equal(mutationScenario(manifest, lastUnit),
        'tracks/ecommerce/scenarios/01-last-unit-2.3.0.json');
      assert.deepEqual(mutationTargetKeys(lastUnit), ['201:201a', '201:201b', '201:201c']);

      const purchase = mutations.get('purchase-does-not-reserve-stock-restock-race');
      assert.equal(mutationScenario(manifest, purchase),
        'tracks/ecommerce/scenarios/01-restock-race-2.3.0.json');
      assert.deepEqual(mutationTargetKeys(purchase), ['202:202a']);
      const restock = mutations.get('restock-does-not-increase-stock');
      assert.equal(mutationScenario(manifest, restock),
        'tracks/ecommerce/scenarios/01-restock-race-2.3.0.json');
      assert.deepEqual(mutationTargetKeys(restock), ['202:202-control', '202:202a']);

      for (const mutation of manifest.mutations) {
        const scenario = mutationScenario(manifest, mutation)
          .replace('tracks/ecommerce/', '');
        for (const key of mutationTargetKeys(mutation)) {
          const split = key.indexOf(':');
          assert(release.checkCatalog.some(check => check.source === scenario
            && String(check.featureId) === key.slice(0, split)
            && check.criterionId === key.slice(split + 1)),
          `${mutation.id} must target an exact check in the 2.3 release`);
        }
        const source = readFileSync(join(app, mutation.file), 'utf8');
        for (const edit of mutationEdits(mutation)) {
          assert.equal(source.split(edit.find).length - 1, 1,
            `${mutation.id} anchor must match exactly once`);
          const mutated = source.replace(edit.find, edit.replace);
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
