import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { REPOSITORY_ROOT, STACK_BENCH_ROOT, findStackBenchRoot }
  from '../src/package-root.js';

test('compiled modules resolve the Stack Bench package and repository roots', () => {
  const manifest = JSON.parse(readFileSync(join(STACK_BENCH_ROOT, 'package.json'), 'utf8')) as {
    name?: unknown;
  };
  assert.equal(manifest.name, '@spacetimedb/stack-bench');
  assert.equal(join(REPOSITORY_ROOT, 'tools', 'stack-bench'), STACK_BENCH_ROOT);
  assert.equal(findStackBenchRoot(import.meta.url), STACK_BENCH_ROOT);
});
