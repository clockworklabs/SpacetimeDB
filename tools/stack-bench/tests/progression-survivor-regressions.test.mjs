import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

const ROOT = join(import.meta.dirname, '..');
const readJson = (...parts) => JSON.parse(readFileSync(join(ROOT, ...parts), 'utf8'));

function postgresMutationSource(id, file) {
  const manifest = readJson('grader', 'mutations', 'postgres-ecom-progression-1.0.0.json');
  const mutation = manifest.mutations.find(candidate => candidate.id === id);
  assert(mutation, `missing PostgreSQL mutation ${id}`);
  let source = readFileSync(join(ROOT, 'reference-apps', 'ecommerce', 'progression',
    'postgres', ...file.split('/')), 'utf8');
  for (const edit of mutation.edits.filter(candidate => (candidate.file ?? mutation.file) === file)) {
    assert.equal(source.split(edit.find).length - 1, 1, `${id} anchor must match once`);
    source = source.replace(edit.find, edit.replace);
  }
  return source;
}

function criterion(source, id) {
  const scenario = compileScenarioDefinition(source);
  return scenario.features.flatMap(feature => feature.criteria)
    .find(candidate => candidate.id === id);
}

test('scheduled-restock access targets the live restock through the named action', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios', '03-deferred-access-1.0.0.json');
  assert.equal(scenario.features[0].setup.some(step => step.do === 'click'
    && step.testid === 'pending-restock-cancel'), false,
  'setup must not leave an earlier DELETE for replayAs to select');

  const replay = criterion(scenario, '317a').steps.find(step => step.do === 'replayAs');
  assert.equal(replay.namedAction.id, 'cancelScheduledRestock');
  assert.deepEqual(replay.namedTarget, {
    testid: 'pending-restock-item',
    contains: 'Webcam',
    attribute: 'data-entity-id',
    valueType: 'string',
  });
});

test('stock-alert uniqueness waits for the second restock update before counting', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios',
    'progression-stock-alerts-1.0.0.json');
  const steps = criterion(scenario, '631a').steps;
  const secondRestock = steps.filter(step => step.do === 'click'
    && step.testid === 'restock-submit')[0];
  const finalCount = steps.at(-1);

  assert(secondRestock.settleMs >= 1000);
  assert.equal(finalCount.do, 'expectElementCount');
  assert.equal(finalCount.equals, 1);
});

test('the duplicate-payment mutation remains visible through the payment view', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios',
    'progression-core-business-1.0.0.json');
  const feature = scenario.features.find(candidate => candidate.id === 623);
  assert.equal(feature.setup.at(-3).do, 'callConcurrently');
  assert.equal(feature.setup.at(-2).do, 'expectCallOutcomes');
  assert.equal(feature.setup.at(-1).testid, 'orders-toggle');
  assert.equal(criterion(scenario, '623a').steps.some(step => step.do === 'callConcurrently'), false);
  assert.equal(criterion(scenario, '623b').steps.some(step => step.do === 'callConcurrently'), false);

  const manifest = readJson('grader', 'mutations', 'spacetime-ecom-progression-1.0.0.json');
  const mutation = manifest.mutations.find(candidate =>
    candidate.id === 'checkout-records-duplicate-payments');
  assert(mutation);
  assert.match(mutation.edits[0].replace,
    /amount: order\.total \+ 0\.01, status: 'paid'/);
});

test('cart expiration waits for the durable expiration state after restart', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios',
    '03-deferred-durability-1.0.0.json');
  const steps = criterion(scenario, '316a').steps;
  assert.equal(steps[0].do, 'reload');
  assert.equal(steps[1].testid, 'cart-toggle');
  assert.equal(steps[2].testid, 'cart-count');
  assert.equal(steps[2].within, 220000);
  assert.equal(steps[3].testid, 'cart-expired-notice');
  assert.equal(steps[4].testid, 'item-stock');
});

test('PostgreSQL cart live-update mutation changes the route used by the owned check', () => {
  const source = postgresMutationSource('progression-cart-update-uses-wrong-room',
    'server/src/index.ts');
  assert.match(source, /await reserveCartItem\(accountId, itemId, qty\);[\s\S]*?io\.to\(`account:\$\{accountId\}:mutation`\)\.emit\("cart:update", state\);/);
});

test('PostgreSQL catalog mutations isolate product name and variant failures', () => {
  const nameSource = postgresMutationSource('progression-catalog-product-name-is-not-published',
    'client/src/App.tsx');
  assert.match(nameSource, /item\.name === "Travel Mug" \? "" : item\.name/);
  assert.match(nameSource, /item\.variants\?\.map/);

  const variantsSource = postgresMutationSource('progression-catalog-variants-are-discarded',
    'server/src/progression.ts');
  assert.match(variantsSource, /\[name, category, price\.toFixed\(2\), \[\]\]/);
});

test('concurrent cart quantity mutations do not change checkout state', () => {
  const postgres = readJson('grader', 'mutations', 'postgres-ecom-progression-1.0.0.json')
    .mutations.find(candidate => candidate.id === 'progression-concurrent-cart-line-does-not-increment');
  assert.equal(postgres.file, 'client/src/App.tsx');
  assert.equal(postgres.edits[0].replace.trim(), 'value={1}');

  const spacetime = readJson('grader', 'mutations', 'spacetime-ecom-progression-1.0.0.json')
    .mutations.find(candidate => candidate.id === 'existing-cart-line-does-not-increment');
  assert.equal(spacetime.file, 'client/src/components/CartPanel.tsx');
  assert.equal(spacetime.edits[0].replace.trim(), 'value={1}');
});

test('staff role check reloads the saved value on each progression reference', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios',
    'progression-staff-roles-1.0.0.json');
  const steps = criterion(scenario, '621a').steps;
  assert.equal(steps.at(-4).do, 'reload');
  assert.deepEqual(steps.at(-1), {
    do: 'expect', actor: 'admin', testid: 'staff-role-select',
    in: { testid: 'staff-role-row', contains: 'staff' },
    value: 'inventory', within: 10000,
  });

  const mongoPanel = readFileSync(join(ROOT, 'reference-apps', 'ecommerce', 'progression',
    'mongodb', 'client', 'src', 'ProgressionPanel.tsx'), 'utf8');
  assert.match(mongoPanel, /roles\[entry\.username\] \?\? entry\.roles\?\.join\(", "\) \?\? ""/);

  const postgresPanel = readFileSync(join(ROOT, 'reference-apps', 'ecommerce', 'progression',
    'postgres', 'client', 'src', 'ProgressionPanel.tsx'), 'utf8');
  assert.match(postgresPanel, /defaultValue=\{role\.role\}/);

  const spacetimePanel = readFileSync(join(ROOT, 'reference-apps', 'ecommerce', 'progression',
    'spacetime', 'client', 'src', 'components', 'ProgressionWorkbench.tsx'), 'utf8');
  assert.match(spacetimePanel, /defaultValue=\{row\.role\}/);

  const source = postgresMutationSource('progression-staff-role-update-is-disabled',
    'server/src/progression.ts');
  assert.doesNotMatch(source, /app\.put\("\/api\/staff\/:id\/role",/);
  assert.match(source, /app\.put\("\/api\/mutation-disabled-staff\/:id\/role",/);
});

test('PostgreSQL reservation mutation remains valid SQL and changes only stock reduction', () => {
  const source = postgresMutationSource('progression-reservation-does-not-reduce-stock',
    'server/src/progression.ts');
  assert.match(source,
    /UPDATE stock SET quantity = quantity \+ \(\$1 \* 0\) WHERE item_id = \$2 AND warehouse_id = \$3`,[\s\S]*?\[take, itemId, row\.warehouse_id\]/);
});
