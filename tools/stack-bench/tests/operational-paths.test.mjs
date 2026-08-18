import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { operationalOutputRoot } from '../src/runtime/operational-paths.mjs';

const STACK_BENCH_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

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
    join(STACK_BENCH_ROOT, 'commands', 'archive-transcripts.mjs'),
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
