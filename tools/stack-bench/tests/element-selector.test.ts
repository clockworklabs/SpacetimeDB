import assert from 'node:assert/strict';
import test from 'node:test';

import { stableElementSelector } from '../src/actions/element-selector.js';

test('stable element selectors support ids used by grading hooks', () => {
  assert.equal(stableElementSelector('account-name'),
    '[data-testid="account-name"],#account-name');
  assert.throws(() => stableElementSelector('account name'), /invalid stable element id/);
});
