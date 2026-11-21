import assert from 'node:assert/strict';
import test from 'node:test';
import { parseExactImageReference, parseImageId, resolveContainerImage }
  from '../src/runtime/container-image.js';

const ID = `sha256:${'a'.repeat(64)}`;

test('malformed Docker image ids are rejected', () => {
  assert.throws(() => parseImageId('stack-bench-build:latest'), /invalid image content id/);
  assert.throws(() => resolveContainerImage('', () => ID), /reference is required/);
});

test('exact image references expose their content digest', () => {
  const reference = `registry.example/stack-bench@${ID}`;
  assert.deepEqual(parseExactImageReference(reference), { reference, id: ID });
  assert.equal(parseExactImageReference('stack-bench-build:latest'), null);
  assert.equal(parseExactImageReference(ID), null, 'signed releases require a registry locator');
  assert.equal(parseExactImageReference(`https://registry.example/app@${ID}`), null);
});
