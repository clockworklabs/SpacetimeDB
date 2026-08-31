import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition } from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));
const readPack = (name: string) => compilePackDefinition(readJson(join(packRoot, name)), { source: name });

const cases: ReadonlyArray<readonly [string, string, readonly string[], readonly string[]]> = [
  ['feature-purchasing-1.2.1.json', 'progression-purchasing-1.0.0.json', ['buyer'], ['3c']],
  ['feature-cart-2.0.0.json', 'progression-cart-checkout-1.0.0.json',
    ['quantity', 'checkout'], ['4a']],
  ['feature-checkout-2.0.0.json', 'progression-cart-checkout-1.0.0.json',
    ['quantity', 'checkout'], ['4d']],
];

test('purchasing, cart, and checkout packs use focused non-prescriptive contracts', () => {
  for (const [packName, scenarioName, actors, criteria] of cases) {
    const pack = readPack(packName);
    assert.equal(pack.task.requirements.length, 1);
    assert.equal(pack.task.contracts.length, 1);
    for (const fragment of [...pack.task.requirements, ...pack.task.contracts]) {
      assert.deepEqual(fragment.modes, ['fresh', 'upgrade']);
      assert.equal(fragment.from, undefined);
      assert.equal(fragment.until, undefined);
    }
    const requirement = pack.task.requirements[0];
    const check = pack.checks[0];
    assert(requirement);
    assert(check);
    const prompt = readFileSync(join(trackRoot, requirement.path), 'utf8');
    assert.doesNotMatch(prompt, /POST \/|reducer|framework|ORM|database|websocket/i);
    assert.equal(check.source, `scenarios/${scenarioName}`);
    assert.deepEqual(check.criteria, criteria);
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, 'scenarios', scenarioName)), {
      source: scenarioName,
      expectedLevel: 1,
    });
    const feature = scenario.features[0];
    assert(feature);
    assert.deepEqual(feature.actors, actors);
    for (const criterion of criteria) {
      assert(feature.criteria.some(candidate => candidate.id === criterion));
    }
  }
});

test('cart criteria use different products so one reservation cannot change the next score', () => {
  const scenario = compileScenarioDefinition(readJson(join(trackRoot, 'scenarios',
    'progression-cart-checkout-1.0.0.json')));
  const feature = scenario.features[0];
  assert(feature);
  const [quantity, checkout] = feature.criteria;
  assert(quantity);
  assert(checkout);
  const product = (criterion: typeof quantity): string => {
    const add = criterion.steps.find(step => step.do === 'click' && step.testid === 'add-to-cart');
    assert(add && typeof add.in?.contains === 'string');
    return add.in.contains;
  };
  assert.equal(product(quantity), 'Headphones');
  assert.equal(product(checkout), 'Desk Lamp');
});
