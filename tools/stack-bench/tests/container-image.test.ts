import assert from 'node:assert/strict';
import test from 'node:test';
import { parseExactImageReference, parseImageId, resolveContainerImage }
  from '../src/runtime/container-image.js';

const ID = `sha256:${'a'.repeat(64)}`;

test('container image references resolve to immutable content ids', () => {
  let invocation: { command: string; args: readonly string[] } | null = null;
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

test('exact image references expose their content digest', () => {
  const reference = `registry.example/stack-bench@${ID}`;
  assert.deepEqual(parseExactImageReference(reference), { reference, id: ID });
  assert.equal(parseExactImageReference('stack-bench-build:latest'), null);
});

test('exact image references reject URL schemes', () => {
  assert.equal(parseExactImageReference(`https://registry.example/app@sha256:${'a'.repeat(64)}`), null);
});
