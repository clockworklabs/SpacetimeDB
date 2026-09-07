import assert from 'node:assert/strict';
import test from 'node:test';

import { isExactSemanticVersion } from '../src/semantic-version.js';

test('accepts exact semantic versions used by runtime contracts', () => {
  assert.equal(isExactSemanticVersion('1.2.3'), true);
  assert.equal(isExactSemanticVersion('1.2.3-rc.1+build.7'), true);
  assert.equal(isExactSemanticVersion('v1.2.3'), false);
  assert.equal(isExactSemanticVersion('^1.2.3'), false);
});
