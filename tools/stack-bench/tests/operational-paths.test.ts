import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync,
  writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

import { archiveTranscripts } from '../src/agents/transcript-archive.js';
import { stackBenchResultsRoot } from '../src/runtime/operational-paths.js';

test('configured operational output must be an exact absolute path', () => {
  const packageRoot = resolve(tmpdir(), 'stack-bench-module');
  const absolute = resolve(tmpdir(), 'stack-bench-results');
  assert.equal(stackBenchResultsRoot(packageRoot, {}), resolve(packageRoot, 'results'));
  assert.equal(stackBenchResultsRoot(packageRoot, { STACK_BENCH_RESULTS_DIR: absolute }), absolute);
  assert.throws(
    () => stackBenchResultsRoot('/opt/stack-bench', { STACK_BENCH_RESULTS_DIR: 'results' }),
    /absolute path/,
  );
  assert.throws(
    () => stackBenchResultsRoot('/opt/stack-bench', { STACK_BENCH_RESULTS_DIR: ` ${absolute}` }),
    /surrounding whitespace/,
  );
});

test('transcript archiving includes nested agent sessions without replacing a longer archive', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-archive-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const app = join(root, 'app');
  const storeRoot = join(root, 'store');
  const storeName = resolve(app).replace(/[\\/:]/g, '-').toLowerCase().replace(/^-+/, '');
  const nested = join(storeRoot, storeName, 'session', 'subagents');
  const output = join(root, 'output');
  mkdirSync(nested, { recursive: true });
  writeFileSync(join(nested, 'agent-1.jsonl'), 'first\n');

  const first = archiveTranscripts(app, 'attempt-1', output, storeRoot);
  const archived = join(output, 'attempt-1', 'session__subagents__agent-1.jsonl');
  assert.deepEqual(first, { copied: 1, missing: 0, outputDirectory: output });
  assert.equal(readFileSync(archived, 'utf8'), 'first\n');

  writeFileSync(archived, 'longer preserved archive\n');
  const second = archiveTranscripts(app, 'attempt-1', output, storeRoot);
  assert.equal(second.copied, 0);
  assert.equal(readFileSync(archived, 'utf8'), 'longer preserved archive\n');
});
