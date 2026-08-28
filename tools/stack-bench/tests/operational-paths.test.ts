import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync,
  writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';

import { archiveTranscripts } from '../commands/archive-transcripts.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { operationalOutputRoot } from '../src/runtime/operational-paths.mjs';

test('operational output stays beside the module outside appliance mode', t => {
  const moduleRoot = mkdtempSync(join(tmpdir(), 'stack-bench-module-'));
  t.after(() => rmSync(moduleRoot, { recursive: true, force: true }));
  assert.equal(operationalOutputRoot(moduleRoot, {}), resolve(moduleRoot));
});

test('appliance output uses the configured durable results root', t => {
  const moduleRoot = mkdtempSync(join(tmpdir(), 'stack-bench-module-'));
  const resultsRoot = mkdtempSync(join(tmpdir(), 'stack-bench-results-'));
  t.after(() => rmSync(moduleRoot, { recursive: true, force: true }));
  t.after(() => rmSync(resultsRoot, { recursive: true, force: true }));
  assert.equal(
    operationalOutputRoot(moduleRoot, { STACK_BENCH_RESULTS_DIR: resultsRoot }),
    resolve(resultsRoot),
  );
});

test('configured operational output must be an exact absolute path', () => {
  assert.throws(
    () => operationalOutputRoot('/opt/stack-bench', { STACK_BENCH_RESULTS_DIR: 'results' }),
    /absolute path/,
  );
  const absolute = resolve(tmpdir(), 'stack-bench-results');
  assert.throws(
    () => operationalOutputRoot('/opt/stack-bench', { STACK_BENCH_RESULTS_DIR: ` ${absolute}` }),
    /surrounding whitespace/,
  );
});

test('transcript archiving creates its default directory under durable results', t => {
  const resultsRoot = mkdtempSync(join(tmpdir(), 'stack-bench-results-'));
  const isolatedHome = mkdtempSync(join(tmpdir(), 'stack-bench-home-'));
  t.after(() => rmSync(resultsRoot, { recursive: true, force: true }));
  t.after(() => rmSync(isolatedHome, { recursive: true, force: true }));

  execFileSync(process.execPath, [
    join(STACK_BENCH_ROOT, 'dist', 'commands', 'archive-transcripts.js'),
    '--app', join(resultsRoot, 'no-transcript-app'),
    '--label', 'path-test',
  ], {
    env: {
      ...process.env,
      HOME: isolatedHome,
      USERPROFILE: isolatedHome,
      STACK_BENCH_RESULTS_DIR: resultsRoot,
    },
    stdio: 'pipe',
  });

  assert.equal(existsSync(join(resultsRoot, 'transcripts')), true);
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

  const first = archiveTranscripts({ results: join(root, 'unused'), out: output,
    app, label: 'attempt-1' }, storeRoot);
  const archived = join(output, 'attempt-1', 'session__subagents__agent-1.jsonl');
  assert.deepEqual(first, { copied: 1, missing: 0, outputDirectory: output });
  assert.equal(readFileSync(archived, 'utf8'), 'first\n');

  writeFileSync(archived, 'longer preserved archive\n');
  const second = archiveTranscripts({ results: join(root, 'unused'), out: output,
    app, label: 'attempt-1' }, storeRoot);
  assert.equal(second.copied, 0);
  assert.equal(readFileSync(archived, 'utf8'), 'longer preserved archive\n');
});
