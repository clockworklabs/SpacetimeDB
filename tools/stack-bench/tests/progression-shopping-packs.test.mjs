import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compilePackDefinition } from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));
const readPack = name => compilePackDefinition(readJson(join(packRoot, name)), { source: name });

const cases = [
  ['feature-purchasing-1.2.0.json', 'progression-purchasing-1.0.0.json', ['buyer'], ['3c']],
  ['feature-cart-checkout-1.3.0.json', 'progression-cart-checkout-1.0.0.json',
    ['quantity', 'checkout'], ['4a', '4d']],
];

test('purchasing and cart packs use focused non-prescriptive contracts', () => {
  for (const [packName, scenarioName, actors, criteria] of cases) {
    const pack = readPack(packName);
    assert.equal(pack.state, 'draft');
    assert.equal(pack.task.requirements.length, 1);
    assert.equal(pack.task.contracts.length, 1);
    for (const fragment of [...pack.task.requirements, ...pack.task.contracts]) {
      assert.deepEqual(fragment.modes, ['fresh', 'upgrade']);
      assert.equal(fragment.from, undefined);
      assert.equal(fragment.until, undefined);
    }
    const prompt = readFileSync(join(trackRoot, pack.task.requirements[0].path), 'utf8');
    assert.doesNotMatch(prompt, /POST \/|reducer|framework|ORM|database|websocket/i);
    assert.equal(pack.checks[0].source, `scenarios/${scenarioName}`);
    assert.deepEqual(pack.checks[0].criteria, criteria);
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, 'scenarios', scenarioName)), {
      source: scenarioName,
      expectedLevel: 1,
    });
    assert.deepEqual(scenario.features[0].actors, actors);
    assert.deepEqual(scenario.features[0].criteria.map(criterion => criterion.id), criteria);
  }
});

test('cart criteria use different products so one reservation cannot change the next score', () => {
  const scenario = compileScenarioDefinition(readJson(join(trackRoot, 'scenarios',
    'progression-cart-checkout-1.0.0.json')));
  const [quantity, checkout] = scenario.features[0].criteria;
  const searched = criterion => criterion.steps
    .find(step => step.do === 'fill' && step.testid === 'search-input').text;
  assert.equal(searched(quantity), 'Headphones');
  assert.equal(searched(checkout), 'Desk Lamp');
});
