import assert from 'node:assert/strict';
import test from 'node:test';
import { parseImageId, resolveContainerImage } from '../container-image.mjs';

const ID = `sha256:${'a'.repeat(64)}`;

test('container image references resolve to immutable content ids', () => {
  let invocation = null;
  const result = resolveContainerImage('stack-bench-build:test', (command, args) => {
    invocation = { command, args };
    return `${ID}\n`;
  });
  assert.deepEqual(result, { reference: 'stack-bench-build:test', id: ID });
  assert.deepEqual(invocation, { command: 'docker',
    args: ['image', 'inspect', '--format', '{{.Id}}', 'stack-bench-build:test'] });
});

test('malformed Docker image ids are rejected', () => {
  assert.throws(() => parseImageId('stack-bench-build:latest'), /invalid image content id/);
  assert.throws(() => resolveContainerImage('', () => ID), /reference is required/);
});
