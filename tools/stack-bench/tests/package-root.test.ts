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

test('staged JavaScript modules resolve the source package root from dist', async () => {
  const compiledModuleUrl = new URL('../src/package-root.js', import.meta.url);
  const compiled = await import(compiledModuleUrl.href) as {
    REPOSITORY_ROOT: string;
    STACK_BENCH_ROOT: string;
    findStackBenchRoot(moduleUrl?: string | URL): string;
  };
  assert.equal(compiled.STACK_BENCH_ROOT, STACK_BENCH_ROOT);
  assert.equal(compiled.REPOSITORY_ROOT, REPOSITORY_ROOT);
  assert.equal(compiled.findStackBenchRoot(import.meta.url), STACK_BENCH_ROOT);
});
