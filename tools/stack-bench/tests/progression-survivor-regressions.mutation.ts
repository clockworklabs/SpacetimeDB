import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileScenarioDefinition, type CompiledCriterion }
  from '../src/composition/definition-compiler.js';
import { readMutationManifest, type LoadedMutationDefinition }
  from '../src/evidence/mutation-analysis.js';

const ROOT = STACK_BENCH_ROOT;
const readJson = (...parts: string[]): unknown =>
  JSON.parse(readFileSync(join(ROOT, ...parts), 'utf8'));

function postgresMutationSource(id: string, file: string): string {
  const manifest = mutationManifest('postgres-ecommerce.json');
  const mutation = manifest.mutations.find(candidate => candidate.id === id);
  assert(mutation, `missing PostgreSQL mutation ${id}`);
  let source = readFileSync(join(ROOT, 'reference-apps', 'ecommerce',
    'postgres', ...file.split('/')), 'utf8');
  for (const edit of mutation.edits.filter(candidate => (candidate.file ?? mutation.file) === file)) {
    assert.equal(source.split(edit.find).length - 1, 1, `${id} anchor must match once`);
    source = source.replace(edit.find, edit.replace);
  }
  return source;
}

function criterion(source: unknown, id: string): CompiledCriterion {
  const scenario = compileScenarioDefinition(source);
  const selected = scenario.features.flatMap(feature => feature.criteria)
    .find(candidate => candidate.id === id);
  assert(selected, `scenario must contain criterion ${id}`);
  return selected;
}

test('scheduled-restock access targets the live restock through the named action', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios', '03-deferred-access.json');
  const compiled = compileScenarioDefinition(scenario);
  const feature = compiled.features[0];
  assert(feature, 'the deferred access scenario must contain a feature');
  assert.equal(feature.setup.some(step => step.do === 'click'
    && step.testid === 'pending-restock-cancel'), false,
  'setup must not leave an earlier DELETE for replayAs to select');

  const replay = criterion(scenario, '317a').steps.find(step => step.do === 'replayAs');
  assert(replay, 'criterion 317a must contain a replay action');
  assert(isRecord(replay.namedAction), 'the replay action must name its target action');
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
    'progression-stock-alerts.json');
  const steps = criterion(scenario, '631a').steps;
  const secondRestock = steps.filter(step => step.do === 'click'
    && step.testid === 'restock-submit')[0];
  const finalCount = steps.at(-1);

  assert(secondRestock, 'criterion 631a must contain a second restock');
  assert(typeof secondRestock.settleMs === 'number');
  assert(secondRestock.settleMs >= 1000);
  assert(finalCount, 'criterion 631a must end with a count check');
  assert.equal(finalCount.do, 'expectElementCount');
  assert.equal(finalCount.equals, 1);
});

test('the duplicate-payment mutation remains visible through the payment view', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios',
    'progression-core-business.json');
  const compiled = compileScenarioDefinition(scenario);
  const feature = compiled.features.find(candidate => candidate.id === 623);
  assert(feature, 'the core business scenario must contain feature 623');
  const concurrentCall = feature.setup.at(-3);
  const callOutcome = feature.setup.at(-2);
  const ordersToggle = feature.setup.at(-1);
  assert(concurrentCall && callOutcome && ordersToggle, 'feature 623 must contain its setup');
  assert.equal(concurrentCall.do, 'callConcurrently');
  assert.equal(callOutcome.do, 'expectCallOutcomes');
  assert.equal(ordersToggle.testid, 'orders-toggle');
  assert.equal(criterion(scenario, '623a').steps.some(step => step.do === 'callConcurrently'), false);
  assert.equal(criterion(scenario, '623b').steps.some(step => step.do === 'callConcurrently'), false);

  const manifest = mutationManifest('spacetime-ecommerce.json');
  const mutation = manifest.mutations.find(candidate =>
    candidate.id === 'checkout-records-duplicate-payments');
  assert(mutation);
  const edit = mutation.edits[0];
  assert(edit, 'the duplicate-payment mutation must contain an edit');
  assert.match(edit.replace,
    /amount: order\.total \+ 0\.01, status: 'paid'/);
});

test('cart expiration waits for the durable expiration state after restart', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios',
    '03-deferred-durability.json');
  const steps = criterion(scenario, '316a').steps;
  const [reload, cartToggle, cartCount, expiredNotice, itemStock] = steps;
  assert(reload && cartToggle && cartCount && expiredNotice && itemStock,
    'criterion 316a must contain all durable expiration checks');
  assert.equal(reload.do, 'reload');
  assert.equal(cartToggle.testid, 'cart-toggle');
  assert.equal(cartCount.testid, 'cart-count');
  assert.equal(cartCount.within, 220000);
  assert.equal(expiredNotice.testid, 'cart-expired-notice');
  assert.equal(itemStock.testid, 'item-stock');
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
  const postgres = requiredMutation(mutationManifest('postgres-ecommerce.json')
    .mutations.find(candidate => candidate.id === 'progression-concurrent-cart-line-does-not-increment'),
  'progression-concurrent-cart-line-does-not-increment');
  assert.equal(postgres.file, 'client/src/App.tsx');
  const postgresEdit = postgres.edits[0];
  assert(postgresEdit, 'the PostgreSQL cart mutation must contain an edit');
  assert.equal(postgresEdit.replace.trim(), 'value={1}');

  const spacetime = requiredMutation(mutationManifest('spacetime-ecommerce.json')
    .mutations.find(candidate => candidate.id === 'existing-cart-line-does-not-increment'),
  'existing-cart-line-does-not-increment');
  assert.equal(spacetime.file, 'client/src/components/CartPanel.tsx');
  const spacetimeEdit = spacetime.edits[0];
  assert(spacetimeEdit, 'the SpacetimeDB cart mutation must contain an edit');
  assert.equal(spacetimeEdit.replace.trim(), 'value={1}');
});

test('staff role check reloads the saved value on each progression reference', () => {
  const scenario = readJson('tracks', 'ecommerce', 'scenarios',
    'progression-staff-roles.json');
  const steps = criterion(scenario, '621a').steps;
  const reload = steps.at(-4);
  const savedRole = steps.at(-1);
  assert(reload && savedRole, 'criterion 621a must reload and check the saved role');
  assert.equal(reload.do, 'reload');
  assert.deepEqual(savedRole, {
    do: 'expect', actor: 'admin', testid: 'staff-role-select',
    in: { testid: 'staff-role-row', contains: 'staff' },
    value: 'inventory', within: 10000,
  });

  const mongoPanel = readFileSync(join(ROOT, 'reference-apps', 'ecommerce',
    'mongodb', 'client', 'src', 'ProgressionPanel.tsx'), 'utf8');
  assert.match(mongoPanel, /roles\[entry\.username\] \?\? entry\.roles\?\.join\(", "\) \?\? ""/);

  const postgresPanel = readFileSync(join(ROOT, 'reference-apps', 'ecommerce',
    'postgres', 'client', 'src', 'ProgressionPanel.tsx'), 'utf8');
  assert.match(postgresPanel, /defaultValue=\{role\.role\}/);

  const spacetimePanel = readFileSync(join(ROOT, 'reference-apps', 'ecommerce',
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

function mutationManifest(name: string) {
  return readMutationManifest(join(ROOT, 'grader', 'mutations', name));
}

function requiredMutation(
  mutation: LoadedMutationDefinition | undefined,
  id: string,
): LoadedMutationDefinition {
  if (!mutation) throw new Error(`mutation ${id} is required`);
  return mutation;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
