import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compilePackDefinition } from '../src/composition/composition-compiler.mjs';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readPack = name => compilePackDefinition(
  JSON.parse(readFileSync(join(packRoot, name), 'utf8')), { source: name });
const cases = [
  ['feature-reviews-1.2.0.json', ['fresh', 'upgrade']],
  ['progression-catalog-management-1.0.0.json', ['upgrade']],
  ['progression-payment-records-1.0.0.json', ['upgrade']],
];

test('review, catalog management, and payment packs have dedicated prompt modules', () => {
  const packs = cases.map(([name]) => readPack(name));
  for (const [index, pack] of packs.entries()) {
    assert.equal(pack.state, 'draft');
    assert.equal(pack.task.requirements.length, 1);
    assert.equal(pack.task.contracts.length, 1);
    for (const fragment of [...pack.task.requirements, ...pack.task.contracts]) {
      assert.deepEqual(fragment.modes, cases[index][1]);
      assert.equal(fragment.from, undefined);
      assert.equal(fragment.until, undefined);
    }
    const prompt = readFileSync(join(trackRoot, pack.task.requirements[0].path), 'utf8');
    assert.doesNotMatch(prompt, /POST \/|reducer|framework|ORM|database|websocket/i);
  }
  assert.equal(new Set(packs.map(pack => pack.task.requirements[0].path)).size, packs.length);
  assert.equal(new Set(packs.map(pack => pack.task.contracts[0].path)).size, packs.length);
});
