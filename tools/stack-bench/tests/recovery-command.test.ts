import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('compiled recovery command rejects incomplete requests without a stack trace', () => {
  const command = join(STACK_BENCH_ROOT, 'dist', 'commands', 'recovery.js');
  const result = spawnSync(process.execPath, [command, 'recover-lease', '/private/lease.json'], {
    encoding: 'utf8',
  });
  assert.equal(result.status, 2);
  assert.match(result.stderr, /Usage:/);
  assert.doesNotMatch(result.stderr, /\n\s+at /);
});
