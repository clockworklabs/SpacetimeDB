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
const cases: readonly [name: string, modes: readonly string[]][] = [
  ['feature-reviews-1.2.1.json', ['fresh', 'upgrade']],
  ['feature-warehouse-admin-1.2.1.json', ['fresh', 'upgrade']],
  ['progression-catalog-management-1.0.2.json', ['upgrade']],
  ['progression-payment-records-2.0.0.json', ['upgrade']],
];

test('review, warehouse, catalog management, and payment packs have dedicated prompt modules', () => {
  const packs = cases.map(([name]) => readPack(name));
  const requirementPaths: string[] = [];
  const contractPaths: string[] = [];
  for (const [index, pack] of packs.entries()) {
    const item = cases[index];
    assert.ok(item);
    const [, modes] = item;
    assert.equal(pack.task.requirements.length, 1);
    assert.equal(pack.task.contracts.length, 1);
    for (const fragment of [...pack.task.requirements, ...pack.task.contracts]) {
      assert.deepEqual(fragment.modes, modes);
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
