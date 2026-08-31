import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition } from '../src/composition/composition-compiler.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readPack = (name: string) => compilePackDefinition(
  JSON.parse(readFileSync(join(packRoot, name), 'utf8')), { source: name });
const names = [
  'l3-reservations-features-2.0.0.json',
  'l3-scheduled-restocks-features-1.1.1.json',
  'l3-order-delivery-features-1.1.1.json',
  'l3-cart-expiration-features-2.0.0.json',
];

test('timed feature packs have dedicated upgrade prompts and testing interfaces', () => {
  const packs = names.map(readPack);
  const requirementPaths: string[] = [];
  const contractPaths: string[] = [];
  for (const pack of packs) {
    assert.equal(pack.task.requirements.length, 1);
    assert.equal(pack.task.contracts.length, 1);
    for (const fragment of [...pack.task.requirements, ...pack.task.contracts]) {
      assert.deepEqual(fragment.modes, ['upgrade']);
      assert.equal(fragment.from, undefined);
      assert.equal(fragment.until, undefined);
    }
    const requirement = pack.task.requirements[0];
    const contract = pack.task.contracts[0];
    assert.ok(requirement);
    assert.ok(contract);
    requirementPaths.push(requirement.path);
    contractPaths.push(contract.path);
    const prompt = readFileSync(join(trackRoot, requirement.path), 'utf8');
    assert.doesNotMatch(prompt, /POST \/|reducer|framework|ORM|database|websocket/i);
  }
  assert.equal(new Set(requirementPaths).size, packs.length);
  assert.equal(new Set(contractPaths).size, packs.length);
});
