import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition, compileRecipeFile } from '../src/composition/composition-compiler.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const packNames = [
  'operations-access-features-1.0.0.json',
  'inventory-operations-features-1.2.0.json',
  'returns-pricing-features-1.1.0.json',
];

test('current L2 runs every source owned by its operations feature packs', () => {
  const recipe = compileRecipeFile(join(trackRoot, 'composition', 'recipes', 'sequential-l2-1.6.0.json'),
    { trackRoot });
  const sources = new Set(recipe.execution.map(entry => entry.source));
  for (const name of packNames) {
    const pack = compilePackDefinition(JSON.parse(readFileSync(
      join(trackRoot, 'composition', 'packs', name), 'utf8')), { source: name });
    assert.equal(pack.moduleType, 'feature');
    for (const check of pack.checks) assert(sources.has(check.source), `${name} must run ${check.source}`);
  }
});
