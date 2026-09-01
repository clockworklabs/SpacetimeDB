import assert from 'node:assert/strict';
import test from 'node:test';

import { isExactSemanticVersion, parseVersionedReference } from '../src/semantic-version.js';

test('accepts exact semantic versions', () => {
  assert.equal(isExactSemanticVersion('1.2.3'), true);
  assert.equal(isExactSemanticVersion('1.2.3-beta.1+build.4'), true);
});

test('rejects normalized and incomplete versions', () => {
  for (const value of ['v1.2.3', ' 1.2.3 ', '01.2.3', '1.2']) {
    assert.equal(isExactSemanticVersion(value), false, value);
  }
});

test('parses an exact versioned reference', () => {
  const identifier = (value: string): boolean => /^[a-z][a-z0-9.-]*$/.test(value);
  assert.deepEqual(parseVersionedReference('feature.accounts@1.2.3', identifier),
    { id: 'feature.accounts', version: '1.2.3' });
  assert.equal(parseVersionedReference('Feature@1.2.3', identifier), null);
});
