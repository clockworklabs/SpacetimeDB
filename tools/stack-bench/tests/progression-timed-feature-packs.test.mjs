import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compilePackDefinition } from '../dist/src/composition/composition-compiler.js';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readPack = name => compilePackDefinition(
  JSON.parse(readFileSync(join(packRoot, name), 'utf8')), { source: name });
const names = [
  'l3-reservations-features-1.1.0.json',
  'l3-scheduled-restocks-features-1.1.0.json',
  'l3-order-delivery-features-1.1.0.json',
  'l3-cart-expiration-features-1.1.0.json',
];

test('timed feature packs have dedicated upgrade prompts and testing interfaces', () => {
  const packs = names.map(readPack);
  for (const pack of packs) {
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
  }
  assert.equal(new Set(packs.map(pack => pack.task.requirements[0].path)).size, packs.length);
  assert.equal(new Set(packs.map(pack => pack.task.contracts[0].path)).size, packs.length);
});
