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

test('order cancellation owns one product prompt, testing interface, and focused scenarios', () => {
  const pack = readPack('l2-order-cancellation-features-1.0.0.json');
  assert.equal(pack.state, 'draft');
  assert.equal(pack.task.requirements.length, 1);
  assert.equal(pack.task.contracts.length, 1);
  for (const fragment of [...pack.task.requirements, ...pack.task.contracts]) {
    assert.deepEqual(fragment.modes, ['upgrade']);
    assert.equal(fragment.from, undefined);
    assert.equal(fragment.until, undefined);
  }
  const prompt = readFileSync(join(trackRoot, pack.task.requirements[0].path), 'utf8');
  assert.doesNotMatch(prompt, /POST \/|reducer|framework|ORM|database|websocket/i);
  assert.deepEqual(pack.checks.map(check => check.source), [
    'scenarios/02-order-cancellation-core-1.0.0.json',
    'scenarios/02-order-cancellation-history-1.0.0.json',
  ]);
  for (const check of pack.checks) {
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
      source: check.source,
      expectedLevel: 2,
    });
    assert(scenario.features.some(feature => feature.id === check.feature));
  }
});
