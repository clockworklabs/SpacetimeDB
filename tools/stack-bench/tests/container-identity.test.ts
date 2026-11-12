import assert from 'node:assert/strict';
import test from 'node:test';

import { parseRunningContainerIdentity } from '../src/runtime/container-identity.js';

test('container identity requires one exact running Docker container', () => {
  const id = 'a'.repeat(64);
  assert.deepEqual(parseRunningContainerIdentity('database', `${id} true\n`),
    { name: 'database', id });
  assert.throws(() => parseRunningContainerIdentity('database', `${id} false`),
    /not a running Docker container/);
  assert.throws(() => parseRunningContainerIdentity('database', 'short true'),
    /not a running Docker container/);
});
