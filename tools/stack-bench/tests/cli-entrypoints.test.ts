import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';

// Compiled entry points report bad requests as usage, never as a stack trace.

test('command entry points report invalid arguments without a stack trace', () => {
  for (const [entrypoint, argv, message] of [
    ['preflight.js', [], /preflight: --backend is required/],
    ['recovery.js', ['recover-lease', '/private/lease.json'], /Usage:/],
  ] as const) {
    const command = join(STACK_BENCH_ROOT, 'dist', 'commands', entrypoint);
    const result = spawnSync(process.execPath, [command, ...argv], { encoding: 'utf8' });
    assert.equal(result.status, 2, entrypoint);
    assert.match(result.stderr, message);
    assert.match(result.stderr, /Usage:/);
    assert.doesNotMatch(result.stderr, /\n\s+at /);
  }
});
